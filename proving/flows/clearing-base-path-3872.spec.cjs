// @test-type: e2e:ui — playwright browser flow (clearing-base-path-3872), live surface
/**
 * #3872 — the Clearing CONNECTS at the /clearing mount, in a browser.
 *
 * Jeff, 2026-08-20, on his phone: "when i access on my phone it hangs
 * indefinitely on 'Connecting'". The redirect to the apex path had landed and
 * been reported as done on the strength of `curl -I` returning 308. A 308 is not
 * a room. Nothing exercised the page.
 *
 * Why it hung: cloudflared does not strip the /clearing prefix, and only the page
 * route was mounted there. The socket.io handshake went to <apex>/socket.io/,
 * which is not routed to the Clearing at all — a 404 that socket.io retries
 * silently forever, so the room shows "connecting…" and never errors.
 *
 * This spec is the guard that turns that silence into a red. It asserts what a
 * person sees: the word "connecting…" goes away, and the room renders.
 *
 * RUN (against any mount — the point is it must pass at BOTH)
 *   CLEARING_URL=http://localhost:3470            npx playwright test proving/flows/clearing-base-path-3872.spec.cjs
 *   CLEARING_URL=http://localhost:3470/clearing   npx playwright test proving/flows/clearing-base-path-3872.spec.cjs
 *
 * NEGATIVE PROOF, and its honest limit (#3734). Run against the UNFIXED build at
 * localhost:3480/clearing, the handshake test goes RED — it asks for the prefixed
 * path directly, which is the thing that was missing. The other two stay GREEN,
 * and that is not a bug in them: locally there is no path-routing edge, so the
 * unfixed client's root-absolute /socket.io still lands on the same server. Only
 * the apex, where /socket.io is NOT routed to the Clearing, reproduces the hang.
 * So: the handshake test is the one that catches this class anywhere; the other
 * two catch it only against the real edge. Local green is not proof.
 */
const { test, expect } = require('@playwright/test');

// #4045 — no prod default. With CLEARING_URL unset this spec used to post into the
// LIVE Clearing on :3470 ("flow-probe <ts>", "dupe-check-<ts>") from every pipeline
// run — Jeff watched seven of them land in the room in one hour (2026-09-02, Kade).
// #3615 class: a test brings its own world or refuses. The variant has no Clearing
// yet, so unset = skip, loudly; set it to a variant room to run.
const CLEARING = process.env.CLEARING_URL;
test.skip(!CLEARING, 'CLEARING_URL unset — refusing to write into the live Clearing (#3615); point it at a variant room to run');

test.describe('#3872 the Clearing connects under its mount', () => {
  test('the room stops saying "connecting…"', async ({ page }) => {
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    // Assert on the STATUS ELEMENT, not on body text. The room displays the
    // team's conversation, and this team talks about connecting — asserting on
    // body means the check goes red whenever the word appears in a message,
    // which is a check that cannot tell a dead socket from a chat log. That
    // exact defect is live in login-journey.spec.cjs:232 and is fixed there in
    // this change. 20s is well past the client's 1s reconnect delay.
    await expect(page.locator('#connection-status'))
      .not.toContainText(/connecting…|connecting\.\.\./i, { timeout: 20000 });
    await expect(page.locator('#connection-status')).toContainText(/connected/i);
  });

  test('no request the page makes 404s', async ({ page }) => {
    // The hang was a 404 nobody looked at. Every asset, module and API call the
    // page issues must resolve under the mount it was served from — collecting
    // them here makes a future base-path regression name its own broken URL
    // instead of showing up as a silent spinner.
    const notFound = [];
    page.on('response', (r) => {
      if (r.status() === 404) notFound.push(`${r.status()} ${r.url()}`);
    });
    await page.goto(CLEARING, { waitUntil: 'networkidle' });
    expect(notFound).toEqual([]);
  });

  test('the socket.io handshake is reachable, not a silent 404', async ({ request }) => {
    // Directly, because this is the one request whose failure the UI hides.
    const res = await request.get(`${CLEARING.replace(/\/$/, '')}/socket.io/?EIO=4&transport=polling`);
    expect(res.status()).toBe(200);
  });
});
