/**
 * chorus-domain-dependencies handler — unit tests (#2188).
 */
import { fetchChorusDomainDependencies, type ChorusDomainDependenciesDeps } from '../../src/handlers/chorus-domain-dependencies';

const envelope = (queryName: string, data: unknown, _d: number, extra?: Record<string, unknown>) => ({
  _meta: { query_name: queryName, ...extra }, data,
});

function makeSparql(
  direct: Array<{ dir: string; other: string; label?: string }>,
  shared: Array<{ otherDomain: string; otherLabel?: string; envName: string }>,
) {
  return async (query: string) => {
    if (query.includes('consumes')) {
      return {
        results: {
          bindings: direct.map((d) => ({
            dir: { value: d.dir },
            other: { value: d.other },
            ...(d.label ? { label: { value: d.label } } : {}),
          })),
        },
      };
    }
    if (query.includes('usesEnvironment')) {
      return {
        results: {
          bindings: shared.map((s) => ({
            otherDomain: { value: s.otherDomain },
            ...(s.otherLabel ? { otherLabel: { value: s.otherLabel } } : {}),
            envName: { value: s.envName },
          })),
        },
      };
    }
    return { results: { bindings: [] } };
  };
}

function deps(over: Partial<ChorusDomainDependenciesDeps> = {}): ChorusDomainDependenciesDeps {
  return {
    sparql: makeSparql([], []) as ChorusDomainDependenciesDeps['sparql'],
    resolveSubdomainId: async (n) => `${n}-domain`,
    envelope,
    now: () => 1_000_000,
    ...over,
  };
}

describe('fetchChorusDomainDependencies (#2188)', () => {
  test('both queries empty → zeros', async () => {
    const body = (await fetchChorusDomainDependencies(deps(), 'photos')).body as {
      _meta: { direct_count: number; shared_count: number };
      data: { direct: { consumes: unknown[]; consumedBy: unknown[] }; shared: unknown[] };
    };
    expect(body._meta.direct_count).toBe(0);
    expect(body._meta.shared_count).toBe(0);
    expect(body.data.direct.consumes).toEqual([]);
    expect(body.data.direct.consumedBy).toEqual([]);
    expect(body.data.shared).toEqual([]);
  });

  test('direct consumes + consumedBy partitioned by dir field', async () => {
    const sparql = makeSparql(
      [
        { dir: 'consumes', other: 'https://x#foo', label: 'Foo' },
        { dir: 'consumedBy', other: 'https://x#bar' },
      ],
      [],
    ) as ChorusDomainDependenciesDeps['sparql'];
    const body = (await fetchChorusDomainDependencies(deps({ sparql }), 'photos')).body as {
      data: { direct: { consumes: Array<{ id: string; label: string }>; consumedBy: Array<{ id: string; label: string }> } };
    };
    expect(body.data.direct.consumes).toEqual([{ id: 'foo', label: 'Foo' }]);
    expect(body.data.direct.consumedBy).toEqual([{ id: 'bar', label: 'bar' }]);
  });

  test('direct_count sums both sides', async () => {
    const sparql = makeSparql(
      [
        { dir: 'consumes', other: 'https://x#a' },
        { dir: 'consumes', other: 'https://x#b' },
        { dir: 'consumedBy', other: 'https://x#c' },
      ],
      [],
    ) as ChorusDomainDependenciesDeps['sparql'];
    const body = (await fetchChorusDomainDependencies(deps({ sparql }), 'photos')).body as {
      _meta: { direct_count: number };
    };
    expect(body._meta.direct_count).toBe(3);
  });

  test('shared groups by domain, dedupes envNames', async () => {
    const sparql = makeSparql(
      [],
      [
        { otherDomain: 'https://x#music', envName: 'gathering-db' },
        { otherDomain: 'https://x#music', envName: 'gathering-db' }, // dup — dedup
        { otherDomain: 'https://x#music', envName: 'gathering-fuseki' },
        { otherDomain: 'https://x#videos', otherLabel: 'Videos', envName: 'gathering-db' },
      ],
    ) as ChorusDomainDependenciesDeps['sparql'];
    const body = (await fetchChorusDomainDependencies(deps({ sparql }), 'photos')).body as {
      _meta: { shared_count: number };
      data: { shared: Array<{ domain: string; label: string; sharedVia: string[] }> };
    };
    expect(body._meta.shared_count).toBe(2);
    const music = body.data.shared.find((s) => s.domain === 'music');
    expect(music?.sharedVia).toEqual(['gathering-db', 'gathering-fuseki']);
    const videos = body.data.shared.find((s) => s.domain === 'videos');
    expect(videos?.label).toBe('Videos');
  });

  test('resolveSubdomainId throw → empty envelope with original name', async () => {
    const body = (await fetchChorusDomainDependencies(
      deps({ resolveSubdomainId: async () => { throw new Error('unknown'); } }),
      'photos',
    )).body as { _meta: { direct_count: number }; data: { subdomain: string } };
    expect(body._meta.direct_count).toBe(0);
    expect(body.data.subdomain).toBe('photos');
  });

  test('sparql throw after resolve → empty envelope', async () => {
    const body = (await fetchChorusDomainDependencies(
      deps({ sparql: async () => { throw new Error('fuseki down'); } }),
      'photos',
    )).body as { _meta: { direct_count: number } };
    expect(body._meta.direct_count).toBe(0);
  });

  test('envelope query_name = "domain-dependencies"', async () => {
    const body = (await fetchChorusDomainDependencies(deps(), 'photos')).body as {
      _meta: { query_name: string };
    };
    expect(body._meta.query_name).toBe('domain-dependencies');
  });
});
