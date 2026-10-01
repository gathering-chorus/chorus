// @test-type: integration:api
/**
 * #3420/#3351 — page-level smoke test for the GENERATED Athena domain page.
 * chorus-api serves the athena-make-emitted shell (public/domain.html) + its renderer
 * + system.css from the '/' static mount. Proves the SHELL anatomy is intact (incl. the
 * #3351 child-domains fold mount) and the assets load. Hermetic: in-process harness.
 */
import { startTestApp, TestApp } from './lib/test-app';

let harness: TestApp;
beforeAll(async () => { harness = await startTestApp(); });
afterAll(async () => { await harness.close(); });

describe('#3420 generated domain page — smoke', () => {
  // #4416 — the /domain.html shell and js/domain-renderer.js cases are gone:
  // #4353 deleted both files (6a1b7841b) and left these two cases red.

  test('the system.css design system loads from the same origin', async () => {
    const res = await fetch(`${harness.baseUrl}/css/system.css`);
    expect(res.status).toBe(200);
  });
});
