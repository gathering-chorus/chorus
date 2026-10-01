// @test-type: e2e:ui — playwright browser flow (clearing-room-key-3865), live surface
/**
 * #3865 — Jeff's browser key must actually generate.
 *
 * `room-key.js` is a module that imports the vendored esm build of
 * @noble/curves. That build keeps BARE specifiers internally
 * ('@noble/hashes/sha2.js'), which a browser cannot resolve — so the module
 * throws before its first statement and `window.roomKey` never exists.
 *
 * Observed twice on 2026-08-13, independently: in the live console in Jeff's
 * browser at 16:51, and by Kade's #3857 UI flow at 19:27.
 *
 * NEGATIVE PROOF (#3734): these must be shown RED against a Clearing WITHOUT
 * the import map, by running them — not asserted. The failure mode this guards
 * against is a flow that only checks the page renders: the page rendered fine
 * all day while the key was dead, so "it loads" cannot be the assertion.
 */
const { test, expect } = require('@playwright/test');
const { ownClearing } = require('./lib/own-clearing.cjs');

// #4045 — no prod default. With CLEARING_URL unset this spec used to post into the
// LIVE Clearing on :3470 ("flow-probe <ts>", "dupe-check-<ts>") from every pipeline
// run — Jeff watched seven of them land in the room in one hour (2026-09-02, Kade).
// #3615 class: a test brings its own world or refuses. The variant has no Clearing
// yet, so unset = skip, loudly; set it to a variant room to run.
// #4417 — unset CLEARING_URL used to skip this spec on every nightly. Now the
// spec starts its own Clearing (lib/own-clearing.cjs); the live room is refused.
const WEBID = 'https://pods.example/jeff/profile/card#me';
const CLEARING_TARGET = ownClearing(test, { signedInAs: WEBID });
const CLEARING = CLEARING_TARGET.url;

test.describe('#3865 — the pod-held key loads', () => {
  test('the page loads with no console exception', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 20000 });
    // Modules execute after DOMContentLoaded; give the import a beat to fail.
    await page.waitForTimeout(1500);

    // Named explicitly rather than "no errors at all" — an unrelated console
    // error should not mask, or fake, this one.
    const specifier = errors.filter((e) => /resolve module specifier/i.test(e));
    expect(specifier, `module specifier errors: ${JSON.stringify(errors)}`).toEqual([]);
  });

  test('the vendored dependency actually serves', async ({ request }) => {
    // If this 404s, the import map would map to nothing and the test above
    // would still pass on a page that simply never got that far. A guard whose
    // target is missing must fail loudly, not vacuously.
    const res = await request.get(`${CLEARING}/vendor/@noble/hashes/sha2.js`);
    expect(res.status()).toBe(200);
  });

  test('room-key.js runs to completion — window.roomKey exists', async ({ page }) => {
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 20000 });
    // The module signals it executed. A module that throws on its import line
    // never reaches this, which is exactly the state that shipped.
    await expect
      .poll(() => page.evaluate(() => typeof window.roomKey), { timeout: 15000 })
      .not.toBe('undefined');
  });
});

// #4417 — moved from room-bind-3827, which matched source text. Signed in, the
// page binds this browser's key to the WebID on load (autoJoin); signed out it
// mints nothing and says so.
test.describe('#3827 the page joins the room on load', () => {
  test('signed in: the page POSTs the bind on load and reports bound', async ({ page, context }) => {
    test.skip(!CLEARING_TARGET.own, 'needs a session in the Clearing this spec started');
    const u = new URL(CLEARING);
    await context.addCookies([{ name: 'clearing_session', value: CLEARING_TARGET.session(), domain: u.hostname, path: '/' }]);
    const bind = page.waitForRequest((r) => r.url().endsWith('/api/room/bind') && r.method() === 'POST', { timeout: 15000 });
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    const req = await bind;
    expect(JSON.parse(req.postData() || '{}').pubkey).toMatch(/^[0-9a-f]{64}$/);
    await expect.poll(() => page.evaluate(() => window.roomKeyStatus && window.roomKeyStatus.state), { timeout: 15000 }).toBe('bound');
  });

  test('NEGATIVE PROOF: a refused bind is reported as failed, never swallowed', async ({ page, context }) => {
    test.skip(!CLEARING_TARGET.own, 'needs a session in the Clearing this spec started');
    const u = new URL(CLEARING);
    await context.addCookies([{ name: 'clearing_session', value: CLEARING_TARGET.session(), domain: u.hostname, path: '/' }]);
    await page.route('**/api/room/bind', (route) => route.fulfill({ status: 500, body: 'stub refused' }));
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await expect.poll(() => page.evaluate(() => window.roomKeyStatus && window.roomKeyStatus.state), { timeout: 15000 }).toBe('failed');
  });

  test('NEGATIVE PROOF: signed out, no key is minted and nothing is POSTed', async ({ page }) => {
    const binds = [];
    page.on('request', (r) => { if (r.url().endsWith('/api/room/bind')) binds.push(r); });
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await expect.poll(() => page.evaluate(() => window.roomKeyStatus && window.roomKeyStatus.state), { timeout: 15000 }).toBe('signed-out');
    expect(binds).toHaveLength(0);
  });
});
