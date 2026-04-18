/**
 * chorus-seeds handler — unit tests (#2189).
 *
 * GET /api/chorus/seeds queries Fuseki's jb:seeds graph and returns up to 50
 * seed entries. Tests verify the response shape Jeff sees:
 *   - successful binding → 200 + { seeds, total }
 *   - Fuseki non-2xx → 502 + { error, status }
 *   - fetch throws → 500 + { error, detail }
 *   - empty bindings → 200 + empty array + total=0
 *   - content > 200 chars → truncated to 200
 *   - status field absent → defaults to 'pending'
 */
import { fetchSeeds, type FetchFn } from '../../src/handlers/chorus-seeds';

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

describe('fetchSeeds (#2189 /api/chorus/seeds)', () => {
  test('binding list → 200 + seeds + total', async () => {
    const r = await fetchSeeds({
      fetchFn: mockFetch({
        body: {
          results: {
            bindings: [
              {
                slug: { value: 'wind-chime' },
                content: { value: 'a seed about chimes' },
                seedUrl: { value: 'https://example.com/x' },
                linkTitle: { value: 'Chime' },
                seededAt: { value: '2026-04-18' },
                routedTo: { value: 'wren' },
              },
              { slug: { value: 'other' } },
            ],
          },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(200);
    const b = r.body as { seeds: unknown[]; total: number };
    expect(b.total).toBe(2);
    expect(b.seeds).toHaveLength(2);
    const first = b.seeds[0] as Record<string, unknown>;
    expect(first.slug).toBe('wind-chime');
    expect(first.routedTo).toBe('wren');
    expect(first.status).toBe('pending');
  });

  test('Fuseki returns non-ok → 502 with status', async () => {
    const r = await fetchSeeds({
      fetchFn: mockFetch({ ok: false, status: 503 }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(502);
    expect(r.body).toEqual({ error: 'Fuseki query failed', status: 503 });
  });

  test('fetch throws → 500 with detail', async () => {
    const r = await fetchSeeds({
      fetchFn: mockFetch({ reject: new Error('ECONNREFUSED') }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(500);
    const b = r.body as { error: string; detail: string };
    expect(b.error).toBe('Seeds query failed');
    expect(b.detail).toBe('ECONNREFUSED');
  });

  test('empty bindings → 200 + empty array + total=0', async () => {
    const r = await fetchSeeds({
      fetchFn: mockFetch({ body: { results: { bindings: [] } } }),
      fusekiUrl: 'http://fake',
    });
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ seeds: [], total: 0 });
  });

  test('content > 200 chars is truncated to 200', async () => {
    const long = 'x'.repeat(500);
    const r = await fetchSeeds({
      fetchFn: mockFetch({
        body: {
          results: {
            bindings: [{ slug: { value: 'long' }, content: { value: long } }],
          },
        },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { seeds: Array<{ content: string }> };
    expect(b.seeds[0].content.length).toBe(200);
  });

  test('status absent → defaults to pending', async () => {
    const r = await fetchSeeds({
      fetchFn: mockFetch({
        body: { results: { bindings: [{ slug: { value: 'x' } }] } },
      }),
      fusekiUrl: 'http://fake',
    });
    const b = r.body as { seeds: Array<{ status: string }> };
    expect(b.seeds[0].status).toBe('pending');
  });
});
