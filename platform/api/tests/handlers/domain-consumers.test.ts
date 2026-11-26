// @test-type: unit — stubs the store; no Fuseki, no live service, brings its own world.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
// #5045 AC3 — "consumed by" counts the real readers. Jeff's experience under test:
// the principles page said nothing consumes principles while chorus-hooks reads them
// on every session start. The read counted only Domain rows on the consumes side, so
// a service that reads a domain was never listed.
import { fetchChorusDomainDependencies } from '../../src/handlers/chorus-domain-dependencies';

const C = 'https://jeffbridwell.com/chorus#';
const envelope = (_n: string, data: unknown, _ms: number, extra?: Record<string, unknown>) => ({ data, _meta: extra });

type Row = { dir: string; other: string; via: string; types?: string; label?: string };
type Body = {
  data: { direct: { consumedBy: Array<{ id: string; label: string; kind?: string }> } };
  _meta: { direct_count: number };
};

/** The store answers the direct read with these rows; every other read is empty. */
function store(rows: Row[]) {
  return (async (q: string) => {
    if (!q.includes('chorus:dependsOn')) return { results: { bindings: [] } };
    return {
      results: {
        bindings: rows.map((r) =>
          Object.fromEntries(Object.entries(r).map(([k, v]) => [k, { value: v as string }])),
        ),
      },
    };
  }) as never;
}

describe('#5045 AC3 — consumed by counts services as well as domains', () => {
  test('a service that consumes the domain is listed, and says it is a service', async () => {
    const r = await fetchChorusDomainDependencies({
      sparql: store([{ dir: 'consumedBy', other: C + 'chorus-hooks', via: 'consumes', types: C + 'Service', label: 'chorus-hooks' }]),
      envelope,
    }, 'principles');
    expect((r.body as Body).data.direct.consumedBy).toEqual([{ id: 'chorus-hooks', label: 'chorus-hooks', kind: 'service' }]);
  });

  test('a domain that consumes the domain is still listed, as a domain', async () => {
    const r = await fetchChorusDomainDependencies({
      sparql: store([{ dir: 'consumedBy', other: C + 'roles', via: 'consumes', types: C + 'Domain' }]),
      envelope,
    }, 'principles');
    expect((r.body as Body).data.direct.consumedBy).toEqual([{ id: 'roles', label: 'roles', kind: 'domain' }]);
  });

  test('NEGATIVE: a consumes edge from a row that is neither a domain nor a service is not a reader', async () => {
    const r = await fetchChorusDomainDependencies({
      sparql: store([
        { dir: 'consumedBy', other: C + 'card-5045', via: 'consumes', types: C + 'Card' },
        { dir: 'consumedBy', other: C + 'untyped-thing', via: 'consumes' },
      ]),
      envelope,
    }, 'principles');
    expect((r.body as Body).data.direct.consumedBy).toEqual([]);
  });

  test('a dependsOn edge counts whatever the type of the row that holds it', async () => {
    const r = await fetchChorusDomainDependencies({
      sparql: store([
        { dir: 'consumedBy', other: C + 'security', via: 'dependsOn', types: C + 'Domain' },
        { dir: 'consumedBy', other: C + 'loki', via: 'dependsOn' },
      ]),
      envelope,
    }, 'principles');
    expect((r.body as Body).data.direct.consumedBy.map((e) => e.id)).toEqual(['security', 'loki']);
  });

  test('NEGATIVE: a domain nothing reads still says none', async () => {
    const r = await fetchChorusDomainDependencies({ sparql: store([]), envelope }, 'board');
    expect((r.body as Body).data.direct.consumedBy).toEqual([]);
    expect((r.body as Body)._meta.direct_count).toBe(0);
  });

  test('NEGATIVE: the count in the envelope is the readers listed, not the edges seen', async () => {
    const r = await fetchChorusDomainDependencies({
      sparql: store([
        { dir: 'consumedBy', other: C + 'chorus-hooks', via: 'consumes', types: C + 'Service' },
        { dir: 'consumedBy', other: C + 'card-5045', via: 'consumes', types: C + 'Card' },
      ]),
      envelope,
    }, 'principles');
    expect((r.body as Body)._meta.direct_count).toBe(1);
  });
});
