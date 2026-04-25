/* global RequestInit */
/**
 * codebase-topology handler — unit tests (#2173 AC4).
 *
 * First pure-function handler test under the decomposition pattern. No
 * harness, no real HTTP, no network. Tests describe behavior Jeff would
 * notice if broken (upstream outage → 502, upstream 404 passthrough,
 * upstream 200 returns the json), not wiring details.
 *
 * The prior HTTP-integration version of this test needed the server on
 * :3340 + the upstream on :3000 + took ~1s per assertion. This file runs
 * regardless of what services are up.
 */

import { fetchTopology } from '../../src/handlers/codebase-topology';

function mockResponse(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as unknown as Response;
}

describe('fetchTopology (#2173 AC4 — first handler extraction)', () => {
  test('upstream 200 returns the json body', async () => {
    const payload = { nodes: 42, edges: 120 };
    const fetcher = async () => mockResponse(200, payload);
    const r = await fetchTopology(fetcher);
    expect(r.status).toBe(200);
    expect(r.body).toEqual(payload);
  });

  test('upstream 404 passes through status + error body', async () => {
    const fetcher = async () => mockResponse(404, {});
    const r = await fetchTopology(fetcher);
    expect(r.status).toBe(404);
    expect(r.body).toEqual({ error: 'upstream 404' });
  });

  test('upstream 503 passes through status + error body', async () => {
    const fetcher = async () => mockResponse(503, {});
    const r = await fetchTopology(fetcher);
    expect(r.status).toBe(503);
    expect(r.body).toEqual({ error: 'upstream 503' });
  });

  test('fetcher throws (network error) maps to 502', async () => {
    const fetcher = async () => { throw new Error('ECONNREFUSED'); };
    const r = await fetchTopology(fetcher);
    expect(r.status).toBe(502);
    expect(r.body).toEqual({ error: 'ECONNREFUSED' });
  });

  test('fetcher throws non-Error value maps to 502 with stringified message', async () => {
    const fetcher = async () => { throw 'timeout'; };
    const r = await fetchTopology(fetcher);
    expect(r.status).toBe(502);
    expect(r.body).toEqual({ error: 'timeout' });
  });

  test('calls the chorus-api upstream URL', async () => {
    let seenUrl = '';
    const fetcher = async (url: string) => { seenUrl = url; return mockResponse(200, {}); };
    await fetchTopology(fetcher);
    expect(seenUrl).toBe('http://localhost:3000/api/codebase/topology');
  });

  test('passes AbortSignal with 8s timeout to the fetcher', async () => {
    let seenInit: RequestInit | undefined;
    const fetcher = async (_url: string, init?: RequestInit) => { seenInit = init; return mockResponse(200, {}); };
    await fetchTopology(fetcher);
    expect(seenInit?.signal).toBeDefined();
    expect(seenInit?.signal).toBeInstanceOf(AbortSignal);
  });
});
