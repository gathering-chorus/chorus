// @test-type: e2e:ui — playwright browser flow (chorus-home-3886), live surface
/**
 * #3886 — the chorus home page, and the way out of it.
 *
 * Jeff, 2026-08-14: "the link to the clearing from chorus home does not work.
 * u need playwright automation for chorus."
 *
 * The second sentence is the durable one. Every flow written this week points
 * at the Clearing. The page Jeff lands on FIRST has no browser coverage at all,
 * which is how a dead link sat on it unnoticed.
 *
 * NEGATIVE PROOF (#3734): this is born RED against today's page, and the red
 * must NAME the Clearing link. If it passes on first run it is pointed at the
 * wrong surface — the localhost-green trap that hid a week of defects (#3877).
 *
 * The second test is the one that keeps it useful after tonight: EVERY link is
 * followed, so the next dead one names itself instead of waiting for Jeff.
 */
const { test, expect } = require('@playwright/test');

const BASE = process.env.FLOW_BASE || 'https://lightlifeurbangardens.com';
// 2026-08-24 — these assertions describe the ENTRANCE, which anonymous never
// sees: the public /chorus answers with our own sign-in door (correct posture,
// #3878). Run anonymous they failed as "no link to the Clearing" — a red that
// named the wrong state. Sign in like a person, or skip with the reason.
const { haveCreds, gotoSignedIn } = require('./_auth.cjs');



test.describe('#3886 — chorus home is a way in, not a dead end', () => {
  test.skip(!haveCreds, 'FLOW_USER/FLOW_PASS not set — the entrance needs a sign-in; refusing to report on a page never seen');
  test('the page offers a link to the Clearing', async ({ page }) => {
    await gotoSignedIn(page, `${BASE}/chorus`);
    const link = page.locator('a[href*="clearing"]').first();
    await expect(link, 'no link to the Clearing on the chorus page').toHaveCount(1);
  });

  test('clicking it reaches the room — click, not a URL assertion', async ({ page }) => {
    await gotoSignedIn(page, `${BASE}/chorus`);
    await page.locator('a[href*="clearing"]').first().click();
    // The room, or its sign-in. What must NOT happen is a 404 or an error page.
    await expect(page.locator('body')).not.toContainText('Cannot GET');
    await expect(page.locator('body')).not.toContainText('404');
  });

  test('every link on the page resolves — the next dead one names itself', async ({ page, request }) => {
    await gotoSignedIn(page, `${BASE}/chorus`);
    const hrefs = await page.locator('a[href]').evaluateAll((as) =>
      as.map((a) => a.getAttribute('href')).filter((h) => h && !h.startsWith('#') && !h.startsWith('mailto:')),
    );
    expect(hrefs.length, 'chorus page has no links at all — wrong surface?').toBeGreaterThan(0);

    const dead = [];
    for (const href of hrefs) {
      const url = href.startsWith('http') ? href : new URL(href, `${BASE}/chorus`).toString();
      const res = await request.get(url, { maxRedirects: 5 }).catch(() => null);
      // 401 is a sign-in door, not a dead link. 404/5xx is dead.
      if (!res || res.status() === 404 || res.status() >= 500) {
        dead.push(`${href} → ${res ? res.status() : 'unreachable'}`);
      }
    }
    expect(dead, `dead links on the chorus page:\n${dead.join('\n')}`).toEqual([]);
  });
});
