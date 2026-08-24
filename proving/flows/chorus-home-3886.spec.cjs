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


// #2646 standing posture: Cloudflare Access fronts the public host. Anonymous
// probes see the wall, not the page — link assertions need credentials there.
// Wall detected → typed skip (same pattern as url-topology-3878), never a
// vacuous pass and never a red that reads as a missing link.
async function skipIfAccessWalled(page, test) {
  const body = await page.locator('body').innerText().catch(() => '');
  const url = page.url() || '';
  if (url.includes('cloudflareaccess.com') || /cloudflareaccess|cf-access/i.test(body)) {
    test.skip(true, 'public /chorus behind the Access wall — link checks need credentials');
  }
}

test.describe('#3886 — chorus home is a way in, not a dead end', () => {
  test('the page offers a link to the Clearing', async ({ page }) => {
    await page.goto(`${BASE}/chorus`, { waitUntil: 'domcontentloaded' });
    await skipIfAccessWalled(page, test);
    const link = page.locator('a[href*="clearing"]').first();
    await expect(link, 'no link to the Clearing on the chorus page').toHaveCount(1);
  });

  test('clicking it reaches the room — click, not a URL assertion', async ({ page }) => {
    await page.goto(`${BASE}/chorus`, { waitUntil: 'domcontentloaded' });
    await skipIfAccessWalled(page, test);
    await page.locator('a[href*="clearing"]').first().click();
    // The room, or its sign-in. What must NOT happen is a 404 or an error page.
    await expect(page.locator('body')).not.toContainText('Cannot GET');
    await expect(page.locator('body')).not.toContainText('404');
  });

  test('every link on the page resolves — the next dead one names itself', async ({ page, request }) => {
    await page.goto(`${BASE}/chorus`, { waitUntil: 'domcontentloaded' });
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
