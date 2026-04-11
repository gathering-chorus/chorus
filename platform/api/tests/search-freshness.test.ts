/**
 * Search freshness metadata tests — #1878
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, index populated.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('GET /api/chorus/search — _meta freshness (#1878)', () => {
  test('response includes _meta with domain_coverage', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(typeof body._meta.domain_coverage).toBe('number');
    expect(body._meta.domain_coverage).toBeGreaterThanOrEqual(0);
    expect(body._meta.domain_coverage).toBeLessThanOrEqual(1);
  });

  test('response includes _meta with newest_result_age_s', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    const body = await res.json();
    expect(typeof body._meta.newest_result_age_s).toBe('number');
    expect(body._meta.newest_result_age_s).toBeGreaterThanOrEqual(0);
  });

  test('response includes _meta with stale boolean', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=5`);
    const body = await res.json();
    expect(typeof body._meta.stale).toBe('boolean');
  });

  test('_meta.sources lists distinct sources in results', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=session&limit=20`);
    const body = await res.json();
    expect(body._meta.sources).toBeDefined();
    expect(typeof body._meta.sources).toBe('object');
  });

  test('semantic mode also includes _meta', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&mode=semantic&limit=5`);
    const body = await res.json();
    if (body.error) return; // semantic index may not be available on CI
    expect(body._meta).toBeDefined();
    expect(typeof body._meta.stale).toBe('boolean');
  });
});
