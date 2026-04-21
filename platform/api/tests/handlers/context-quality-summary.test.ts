/**
 * context-quality-summary handler tests (#2252 migration).
 */

import {
  fetchContextQualitySummary,
  type ContextQualitySummaryDeps,
} from '../../src/handlers/context-quality-summary';

function stubSparql(): ContextQualitySummaryDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextQualitySummary', () => {
  it('wraps quality-summary body in common envelope', async () => {
    const innerBody = { total: 7534, pyramid: [{ name: 'Unit', count: 5000 }] };
    const r = await fetchContextQualitySummary(
      {
        sparql: stubSparql(),
        runQuality: async () => ({ status: 200, body: innerBody }),
      },
      '/api/chorus/context/quality/summary',
    );
    expect(r.status).toBe(200);
    const body = r.body as { source: string; domain?: string; data: typeof innerBody };
    expect(body.source).toBe('/api/chorus/context/quality/summary');
    expect(body.domain).toBe('chorus');
    expect(body.data.total).toBe(7534);
    expect(body.data.pyramid).toHaveLength(1);
  });

  it('propagates non-200 status', async () => {
    const r = await fetchContextQualitySummary(
      {
        sparql: stubSparql(),
        runQuality: async () => ({ status: 503, body: { error: 'db unavailable' } }),
      },
      '/api/chorus/context/quality/summary',
    );
    expect(r.status).toBe(503);
  });

  it('envelope has ISO timestamp', async () => {
    const r = await fetchContextQualitySummary(
      {
        sparql: stubSparql(),
        runQuality: async () => ({ status: 200, body: {} }),
      },
      '/api/chorus/context/quality/summary',
    );
    expect((r.body as { timestamp: string }).timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T.*Z$/);
  });
});
