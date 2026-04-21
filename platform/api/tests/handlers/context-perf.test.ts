/**
 * context-perf handler tests (#2252 migration).
 */

import {
  fetchContextPerf,
  type ContextPerfDeps,
} from '../../src/handlers/context-perf';

function stubSparql(): ContextPerfDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextPerf', () => {
  it('wraps perf body in common envelope', async () => {
    const innerBody = { p95: 120, p99: 240, runs: 50, timestamp: '2026-04-21' };
    const r = await fetchContextPerf(
      {
        sparql: stubSparql(),
        runPerf: async () => ({ status: 200, body: innerBody }),
      },
      '/api/chorus/context/perf',
    );
    expect(r.status).toBe(200);
    const body = r.body as { source: string; domain?: string; data: typeof innerBody };
    expect(body.source).toBe('/api/chorus/context/perf');
    expect(body.domain).toBe('chorus');
    expect(body.data.p95).toBe(120);
  });

  it('propagates non-200 status', async () => {
    const r = await fetchContextPerf(
      {
        sparql: stubSparql(),
        runPerf: async () => ({ status: 500, body: { error: 'perf script missing' } }),
      },
      '/api/chorus/context/perf',
    );
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toMatch(/perf script/);
  });

  it('envelope has ISO timestamp', async () => {
    const r = await fetchContextPerf(
      {
        sparql: stubSparql(),
        runPerf: async () => ({ status: 200, body: {} }),
      },
      '/api/chorus/context/perf',
    );
    expect((r.body as { timestamp: string }).timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T.*Z$/);
  });
});
