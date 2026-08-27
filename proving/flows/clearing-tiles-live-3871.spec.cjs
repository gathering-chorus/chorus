/**
 * #3871 — the role tiles must be TRUE on Jeff's surface, in a tab he leaves open.
 *
 * His phone, 2026-08-14 16:59: "Silas idle 27m, Kade idle 30m". At that same
 * moment `:3470/api/tiles` said both were building, seconds ago. The data was
 * right and his screen was old.
 *
 * So two failures are possible and they need different fixes:
 *   1. the page never updates without a reload  (render/poll)
 *   2. the tunnel serves different values than :3470  (transport)
 *
 * A test that only asserts (1) passes on localhost while his phone lies, which
 * is the defect this card exists to end. Both are asserted.
 */
const { test, expect } = require('@playwright/test');

const BASE = process.env.FLOW_BASE || 'http://localhost:3470';
const LOCAL = 'http://localhost:3470';

test.describe('#3871 — tiles are true where Jeff looks', () => {
  test('a long-lived tab updates its tiles WITHOUT a reload', async ({ page, request }) => {
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#tiles', { timeout: 20000 });

    const first = await page.locator('#tiles').innerText();

    // Generate real activity, then wait WITHOUT reloading. If the tab only
    // refreshes on load, this text never changes — which is exactly what Jeff
    // sees after leaving the room open on his phone.
    await request.post(`${BASE}/api/message`, {
      data: { from: 'wren', text: `tile-liveness ${Date.now()}` },
    });

    await expect
      .poll(async () => page.locator('#tiles').innerText(), { timeout: 45000, intervals: [2000] })
      .not.toBe(first);
  });

  test('the public surface serves the SAME tile state as :3470', async ({ request }) => {
    // Skip when already pointed at localhost — comparing a thing to itself
    // proves nothing, and passing for that reason is the hollow shape.
    test.skip(BASE === LOCAL, 'FLOW_BASE is localhost — no second surface to compare');

    const [pub, local] = await Promise.all([
      request.get(`${BASE}/api/tiles`),
      request.get(`${LOCAL}/api/tiles`),
    ]);
    expect(pub.status(), 'public tiles unreachable').toBeLessThan(400);
    expect(local.status()).toBe(200);

    const norm = (rows) =>
      rows.map((t) => `${t.role}:${t.state}`).sort().join('|');
    expect(norm(await pub.json()), 'public tile state differs from :3470')
      .toBe(norm(await local.json()));
  });
});
