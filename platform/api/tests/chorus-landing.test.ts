/**
 * Chorus landing tests — #2099 (follow-on per Jeff's /docs-clinical feedback)
 *
 * Promotes the Chorus product landing from /docs/ to root (/). /docs/ retained
 * as legacy alias until clients are updated. Removing this suite is a signal
 * that the alias can be deleted.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099 follow-on: Chorus landing at /', () => {

  test('GET / returns 200', async () => {
    const res = await fetch(`${API}/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('landing contains Chorus h1 and shaping-surface subtitle', async () => {
    const res = await fetch(`${API}/`);
    const html = await res.text();
    expect(html).toContain('<h1>Chorus</h1>');
    expect(html).toContain('Shaping surfaces');
  }, 10_000);

  test('landing renders product grid via /api/chorus/products', async () => {
    const res = await fetch(`${API}/`);
    const html = await res.text();
    expect(html).toContain('/api/chorus/products');
  }, 10_000);

  test('/docs/ alias preserved (backwards compat)', async () => {
    const res = await fetch(`${API}/docs/`);
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain('<h1>Chorus</h1>');
  }, 10_000);
});
