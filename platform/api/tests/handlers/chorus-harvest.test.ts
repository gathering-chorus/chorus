/**
 * chorus-harvest handler — unit tests (#2189).
 *
 * GET /api/chorus/harvest reports triple counts per graph, rolled up by
 * domain prefix. Tests verify:
 *   - empty bindings → 200 with totals=0 and empty domains
 *   - single graph → domain inferred from graph name prefix
 *   - multiple graphs under same prefix → aggregated into one domain entry
 *   - domains sorted by triple count descending
 *   - Fuseki non-ok response → 500 with detail
 *   - fetch throws → 500 with error string
 *   - graph-name edge cases: '.ttl' suffix stripped; '-' and '_' splitters
 */
import { fetchHarvest, type FetchFn } from '../../src/handlers/chorus-harvest';

function mockFetch(spec: {
  ok?: boolean;
  status?: number;
  body?: unknown;
  reject?: Error;
}): FetchFn {
  return async () => {
    if (spec.reject) throw spec.reject;
    return {
      ok: spec.ok !== false,
      status: spec.status ?? 200,
      json: async () => spec.body ?? { results: { bindings: [] } },
    };
  };
}

function binding(graphUri: string, count: number) {
  return { g: { value: graphUri }, count: { value: String(count) } };
}

describe('fetchHarvest (#2189 /api/chorus/harvest)', () => {
  test('empty bindings → 200 with zero totals and empty domains', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({ body: { results: { bindings: [] } } }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ total_graphs: 0, total_triples: 0, domains: [] });
  });

  test('single graph → domain inferred from prefix', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({
        body: {
          results: { bindings: [binding('urn:graph/music-albums.ttl', 1234)] },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { total_graphs: number; total_triples: number; domains: Array<{ name: string; graphs: number; triples: number }> };
    expect(b.total_graphs).toBe(1);
    expect(b.total_triples).toBe(1234);
    expect(b.domains).toEqual([{ name: 'music', graphs: 1, triples: 1234 }]);
  });

  test('two graphs same prefix → aggregated', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({
        body: {
          results: {
            bindings: [
              binding('urn:graph/music-albums.ttl', 100),
              binding('urn:graph/music-plays.ttl', 50),
            ],
          },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { domains: Array<{ name: string; graphs: number; triples: number }> };
    expect(b.domains).toEqual([{ name: 'music', graphs: 2, triples: 150 }]);
  });

  test('domains sorted by triples descending', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({
        body: {
          results: {
            bindings: [
              binding('urn:graph/music.ttl', 10),
              binding('urn:graph/photos-2024.ttl', 1000),
              binding('urn:graph/notes.ttl', 100),
            ],
          },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { domains: Array<{ name: string }> };
    expect(b.domains.map((d) => d.name)).toEqual(['photos', 'notes', 'music']);
  });

  test('Fuseki non-ok → 500 with detail', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({ ok: false, status: 503 }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'Fuseki query failed', detail: 'Fuseki: 503' });
  });

  test('fetch throws → 500 with error string', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({ reject: new Error('ECONNREFUSED') }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(500);
    const b = r.body as { error: string; detail: string };
    expect(b.error).toBe('Fuseki query failed');
    expect(b.detail).toContain('ECONNREFUSED');
  });

  test('.ttl suffix stripped; underscore also splits', async () => {
    const r = await fetchHarvest({
      fetchFn: mockFetch({
        body: {
          results: {
            bindings: [
              binding('urn:graph/photos_enriched.ttl', 5),
              binding('urn:graph/photos.ttl', 3),
            ],
          },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { domains: Array<{ name: string; graphs: number }> };
    expect(b.domains).toEqual([{ name: 'photos', graphs: 2, triples: 8 }]);
  });
});
