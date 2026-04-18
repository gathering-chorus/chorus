/**
 * Chorus landing tests — #2099 (follow-on per Jeff's /docs-clinical feedback).
 *
 * Promotes the Chorus product landing from /docs/ to root (/). /docs/ retained
 * as legacy alias until clients are updated. Removing this suite is a signal
 * that the alias can be deleted.
 *
 * Converted to in-process harness (#2173 AC4) — no longer gated on
 * RUN_INTEGRATION + live :3340.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099 follow-on: Chorus landing at /', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET / returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/`);
    expect(res.status).toBe(200);
  });

  test('landing contains Chorus h1 and shaping-surface subtitle', async () => {
    const res = await fetch(`${harness.baseUrl}/`);
    const html = await res.text();
    expect(html).toContain('<h1>Chorus</h1>');
    expect(html).toContain('Shaping surfaces');
  });

  test('landing renders product grid via /api/chorus/products', async () => {
    const res = await fetch(`${harness.baseUrl}/`);
    const html = await res.text();
    expect(html).toContain('/api/chorus/products');
  });

  test('/docs/ alias preserved (backwards compat)', async () => {
    const res = await fetch(`${harness.baseUrl}/docs/`);
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain('<h1>Chorus</h1>');
  });
});
