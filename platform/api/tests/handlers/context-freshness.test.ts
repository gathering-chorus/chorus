/**
 * context-freshness handler tests (#2252 migration wave).
 *
 * Covers the envelope-wrapping layer around the existing fetchFreshness
 * output. fetchFreshness itself is covered by its own test suite; this
 * suite asserts the common-envelope contract and the wrapper shape.
 */

import {
  fetchContextFreshness,
  type ContextFreshnessDeps,
} from '../../src/handlers/context-freshness';

function stubSparql(): ContextFreshnessDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextFreshness', () => {
  it('wraps fetchFreshness body in common envelope', async () => {
    const innerBody = {
      sources: [{ source: 'spine', level: 'fresh', age_seconds: 10 }],
      summary: { total_sources: 1, fresh: 1, warn: 0, critical: 0, dead: 0 },
      timestamp: '2026-04-21 10:00:00',
    };
    const r = await fetchContextFreshness(
      {
        sparql: stubSparql(),
        runFreshness: () => ({ status: 200, body: innerBody }),
      },
      '/api/chorus/context/freshness',
    );
    expect(r.status).toBe(200);
    const body = r.body as {
      source: string; timestamp: string; domain?: string;
      data: typeof innerBody;
    };
    expect(body.source).toBe('/api/chorus/context/freshness');
    expect(body.domain).toBe('chorus');
    expect(body.data.sources[0].source).toBe('spine');
    expect(body.data.summary.total_sources).toBe(1);
  });

  it('propagates non-200 status without envelope wrap', async () => {
    const r = await fetchContextFreshness(
      {
        sparql: stubSparql(),
        runFreshness: () => ({ status: 503, body: { error: 'Index database not found' } }),
      },
      '/api/chorus/context/freshness',
    );
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/Index database/);
  });

  it('envelope has ISO timestamp', async () => {
    const r = await fetchContextFreshness(
      {
        sparql: stubSparql(),
        runFreshness: () => ({ status: 200, body: { sources: [], summary: {}, timestamp: '' } }),
      },
      '/api/chorus/context/freshness',
    );
    const body = r.body as { timestamp: string };
    expect(body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T.*Z$/);
  });
});
