// @test-type: unit — pure functions on fixtures; no live services
/**
 * chorus-domain-pipeline handler — unit tests (#2188).
 */
import { fetchChorusDomainPipeline, type ChorusDomainPipelineDeps } from '../../src/handlers/chorus-domain-pipeline';

const envelope = (queryName: string, data: unknown) => ({ _meta: { query_name: queryName }, data });

function makeFetcher(routeMap: Record<string, unknown>): (url: string) => Promise<unknown | null> {
  return async (url: string) => {
    for (const [suffix, body] of Object.entries(routeMap)) {
      // "=<url>" matches that URL exactly; anything else matches as a substring
      if (suffix.startsWith('=') ? url === suffix.slice(1) : url.includes(suffix)) return body;
    }
    return null;
  };
}

function deps(over: Partial<ChorusDomainPipelineDeps> = {}): ChorusDomainPipelineDeps {
  return {
    fetcher: async () => null,
    resolveSubdomainId: async (n) => `${n}-domain`,
    envelope,
    now: () => 1_000_000,
    ...over,
  };
}

describe('fetchChorusDomainPipeline (#2188)', () => {
  test('resolveSubdomainId throws → 200 with empty "not_started" stages + original name', async () => {
    const r = await fetchChorusDomainPipeline(
      deps({ resolveSubdomainId: async () => { throw new Error('unknown'); } }),
      'photos',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { subdomain: string; stages: Array<{ status: string }> } };
    expect(body.data.subdomain).toBe('photos');
    expect(body.data.stages.map((s) => s.status)).toEqual(
      ['not_started', 'not_started', 'not_started', 'not_started', 'not_started'],
    );
  });

  test('all fetches return null → all stages not_started', async () => {
    const body = (await fetchChorusDomainPipeline(deps(), 'photos')).body as {
      data: { stages: Array<{ name: string; status: string }> };
    };
    for (const s of body.data.stages) expect(s.status).toBe('not_started');
  });

  test('shape: 5 cards → complete', async () => {
    const fetcher = makeFetcher({
      '=/api/chorus/domain/photos': { cards: Array(5).fill({ status: 'Next' }) },
    });
    const body = (await fetchChorusDomainPipeline(deps({ fetcher }), 'photos')).body as {
      data: { stages: Array<{ name: string; status: string; evidence: number }> };
    };
    const shape = body.data.stages.find((s) => s.name === 'shape');
    expect(shape?.status).toBe('complete');
    expect(shape?.evidence).toBe(5);
  });

  test('shape: 3 cards → in_progress, 0 → not_started', async () => {
    const bodyThree = (await fetchChorusDomainPipeline(deps({
      fetcher: makeFetcher({ '=/api/chorus/domain/photos': { cards: Array(3).fill({ status: 'Next' }) } }),
    }), 'photos')).body as { data: { stages: Array<{ name: string; status: string }> } };
    expect(bodyThree.data.stages.find((s) => s.name === 'shape')?.status).toBe('in_progress');
  });

  test('design: completeness 80 → complete; 0 → not_started; 40 → in_progress', async () => {
    const cases: Array<[number, string]> = [[80, 'complete'], [0, 'not_started'], [40, 'in_progress']];
    for (const [pct, expected] of cases) {
      const fetcher = makeFetcher({
        '=/api/chorus/domain/photos': { completeness: { percentage: pct, present: [], missing: [] } },
      });
      const body = (await fetchChorusDomainPipeline(deps({ fetcher }), 'photos')).body as {
        data: { stages: Array<{ name: string; status: string }> };
      };
      expect(body.data.stages.find((s) => s.name === 'design')?.status).toBe(expected);
    }
  });

  test('build: code+tests+endpoints ≥3 → complete', async () => {
    const fetcher = makeFetcher({
      '/code': { _meta: { source_count: 2 } },
      '/tests': { _meta: { count: 1 } },
      '/services': { _meta: { count: 3 } },
    });
    const body = (await fetchChorusDomainPipeline(deps({ fetcher }), 'photos')).body as {
      data: { stages: Array<{ name: string; status: string; evidence: number }> };
    };
    const build = body.data.stages.find((s) => s.name === 'build');
    expect(build?.evidence).toBe(6);
    expect(build?.status).toBe('complete');
  });

  test('prove: ≥1 alert → complete; 0 → not_started', async () => {
    const bodyOne = (await fetchChorusDomainPipeline(deps({
      fetcher: makeFetcher({ '/alerts': { _meta: { count: 1 } } }),
    }), 'photos')).body as { data: { stages: Array<{ name: string; status: string }> } };
    expect(bodyOne.data.stages.find((s) => s.name === 'prove')?.status).toBe('complete');

    const bodyZero = (await fetchChorusDomainPipeline(deps(), 'photos')).body as {
      data: { stages: Array<{ name: string; status: string }> };
    };
    expect(bodyZero.data.stages.find((s) => s.name === 'prove')?.status).toBe('not_started');
  });

  test('ship: ≥50% done → complete; 0 → not_started', async () => {
    const fetcher = makeFetcher({
      '=/api/chorus/domain/photos': { cards: [
        { status: 'Done' }, { status: 'Done' }, { status: 'Done' }, { status: 'WIP' },
      ] },
    });
    const body = (await fetchChorusDomainPipeline(deps({ fetcher }), 'photos')).body as {
      data: { stages: Array<{ name: string; status: string; detail: { ratio?: number } }> };
    };
    const ship = body.data.stages.find((s) => s.name === 'ship');
    expect(ship?.status).toBe('complete');
    expect(ship?.detail.ratio).toBe(75);
  });

  test('#4353 NEGATIVE PROOF — never asks a retired /api/athena/subdomains route', async () => {
    const asked: string[] = [];
    const fetcher = async (url: string) => { asked.push(url); return null; };
    await fetchChorusDomainPipeline(deps({ fetcher }), 'photos');
    expect(asked).toContain('/api/chorus/domain/photos');
    expect(asked.filter((u) => u.includes('/api/athena/subdomains'))).toEqual([]);
  });

  test('envelope wraps body with query_name "domain-pipeline"', async () => {
    const body = (await fetchChorusDomainPipeline(deps(), 'photos')).body as {
      _meta: { query_name: string };
    };
    expect(body._meta.query_name).toBe('domain-pipeline');
  });
});
