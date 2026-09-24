// @test-type: unit — stubs the store; no Fuseki, no live service, brings its own world.
// #4293 — the domain page's Dependencies fold. Jeff's experience under test: a
// domain page says which layer the domain sits in, which services it hosts, what
// it depends on and what depends on it, from the graph. Before #4293 the fold read
// urn:chorus:instances, a graph no domain edge lives in, so every domain said
// "none recorded" whatever the model held.
import { fetchChorusDomainDependencies } from '../src/handlers/chorus-domain-dependencies';

const C = 'https://jeffbridwell.com/chorus#';

// A tiny store: triples as [graph, s, p, o]; each query is answered by matching
// the shape of the handler's SPARQL, the way the real store would.
function world(): { sparql: (q: string) => Promise<any>; asked: string[] } {
  const asked: string[] = [];
  const b = (o: Record<string, string>) =>
    Object.fromEntries(Object.entries(o).map(([k, v]) => [k, { value: v }]));
  const sparql = async (q: string) => {
    asked.push(q);
    if (q.includes('chorus:inLayer')) {
      return { results: { bindings: [b({ layer: C + 'layer-foundation', layerLabel: 'foundation', rank: '0' })] } };
    }
    if (q.includes('chorus:hosts')) {
      return { results: { bindings: [b({ svc: C + 'logs-domain-service-loki', svcLabel: 'Loki' })] } };
    }
    if (q.includes('chorus:dependsOn')) {
      return {
        results: {
          bindings: [
            b({ dir: 'consumes', other: C + 'time' }),
            b({ dir: 'consumes', other: C + 'infrastructure' }),
            b({ dir: 'consumedBy', other: C + 'security' }),
            b({ dir: 'consumes', other: C + 'time' }), // same edge from a second graph
          ],
        },
      };
    }
    if (q.includes('rdfs:label') && q.includes('VALUES ?other')) {
      return {
        results: {
          bindings: [
            b({ other: C + 'time', label: 'time' }),
            b({ other: C + 'infrastructure', label: 'infrastructure' }),
            b({ other: C + 'security', label: 'security' }),
          ],
        },
      };
    }
    return { results: { bindings: [] } };
  };
  return { sparql, asked };
}

const envelope = (_n: string, data: unknown, _ms: number, extra?: Record<string, unknown>) => ({ data, _meta: extra });

describe('#4293 domain dependencies read the CMDB edges', () => {
  test('logs shows its layer, the service it hosts, and its dependencies both ways', async () => {
    const w = world();
    const r = await fetchChorusDomainDependencies({ sparql: w.sparql, resolveSubdomainId: async (n) => n, envelope }, 'logs');
    const d = (r.body as any).data;
    expect(d.layer).toEqual({ id: 'layer-foundation', label: 'foundation', rank: 0 });
    expect(d.hosts).toEqual([{ id: 'logs-domain-service-loki', label: 'Loki' }]);
    expect(d.direct.consumes.map((x: any) => x.id).sort()).toEqual(['infrastructure', 'time']);
    expect(d.direct.consumedBy.map((x: any) => x.id)).toEqual(['security']);
  });

  test('the same edge in two graphs is listed once', async () => {
    const w = world();
    const r = await fetchChorusDomainDependencies({ sparql: w.sparql, resolveSubdomainId: async (n) => n, envelope }, 'logs');
    const ids = (r.body as any).data.direct.consumes.map((x: any) => x.id);
    expect(ids.filter((i: string) => i === 'time').length).toBe(1);
  });

  test('NEGATIVE: no query is pinned to urn:chorus:instances, the graph that made every fold read "none recorded"', async () => {
    const w = world();
    await fetchChorusDomainDependencies({ sparql: w.sparql, resolveSubdomainId: async (n) => n, envelope }, 'logs');
    const pinned = w.asked.filter((q) => q.includes('urn:chorus:instances'));
    expect(pinned).toEqual([]);
    expect(w.asked.some((q) => q.includes('chorus:dependsOn'))).toBe(true);
  });

  test('NEGATIVE: a store failure says it failed; an empty fold is never silent', async () => {
    const r = await fetchChorusDomainDependencies(
      { sparql: async () => { throw new Error('store down'); }, resolveSubdomainId: async (n) => n, envelope },
      'logs',
    );
    const body = r.body as any;
    expect(body.data.direct.consumes).toEqual([]);
    expect(body._meta.error).toBe(true);
    expect(body._meta.message).toBe('store down');
  });
});
