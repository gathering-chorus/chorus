/**
 * chorus-health-detail handler — unit tests (#2189).
 *
 * Verifies Ollama status classification, health-cache pass-through, and
 * timestamp injection.
 */
import { fetchHealthDetail, type FetchFn, type HealthDetailDeps } from '../../src/handlers/chorus-health-detail';

const CACHE: HealthDetailDeps['healthCache'] = {
  dbStatus: 'up',
  dbRows: 1234,
  vectors: { count: 99 },
  unembedded: { count: 0 },
  hooksStatus: 'ok',
};

function deps(fetchFn: FetchFn): HealthDetailDeps {
  return {
    fetchFn,
    healthCache: CACHE,
    ollamaUrl: 'http://fake-ollama',
    timestamp: () => '2026-04-18T10:00:00-04:00',
  };
}

describe('fetchHealthDetail (#2189 /api/chorus/health/detail)', () => {
  test('ollama ok → status=up, health-cache fields echoed', async () => {
    const r = await fetchHealthDetail(deps(async () => ({ ok: true, status: 200 })));
    expect(r.status).toBe(200);
    expect(r.body.ollama.status).toBe('up');
    expect(r.body.db).toEqual({ status: 'up', rows: 1234 });
    expect(r.body.vectors).toEqual({ count: 99 });
    expect(r.body.hooks).toEqual({ status: 'ok' });
    expect(r.body.timestamp).toBe('2026-04-18T10:00:00-04:00');
  });

  test('ollama non-2xx → status=degraded', async () => {
    const r = await fetchHealthDetail(deps(async () => ({ ok: false, status: 503 })));
    expect(r.body.ollama.status).toBe('degraded');
  });

  test('fetch throws → status=down (not 500)', async () => {
    const r = await fetchHealthDetail(deps(async () => { throw new Error('timeout'); }));
    expect(r.status).toBe(200);
    expect(r.body.ollama.status).toBe('down');
  });

  test('timestamp taken from injected fn', async () => {
    const r = await fetchHealthDetail({
      ...deps(async () => ({ ok: true, status: 200 })),
      timestamp: () => 'CUSTOM',
    });
    expect(r.body.timestamp).toBe('CUSTOM');
  });
});
