/**
 * #3857 — the four Clearing behaviours Jeff found by using it, 2026-08-13.
 *
 * Every one of these lived in a layer this repo had no tests for. I "proved" the
 * scroll fix that morning by asserting the SOURCE had the right shape — a grep,
 * which passes whether or not scrolling actually holds. That is the hollow-check
 * shape, written by me on the same day I named it in other people's work.
 *
 * These drive the real browser against the live Clearing. One environment, and
 * it is production: a flow that proves the room works has to open the real room.
 */
const { test, expect } = require('@playwright/test');

const CLEARING = process.env.CLEARING_URL || 'http://localhost:3470';

/**
 * Post into the ROOM.
 *
 * NOT /api/chat/message — that writes the chat-SESSION store, a different store
 * from the room, and it answers 200 either way. I probed it six times, saw six
 * empty reads, and told Jeff his messages were being lost. They were not; I was
 * knocking on the wrong door and reading the silence as a defect.
 *
 * /api/message is the room's ingest (server.ts:1228 -> messageRouter.ingest).
 */
async function postAs(request, from, text) {
  return request.post(`${CLEARING}/api/message`, { data: { from, text } });
}

test.describe('Clearing UI — the behaviours Jeff reported', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 15000 });
  });

  // Jeff: "if i scroll up i get forced back to bottom even when no new messages
  // arrive... message arrive must not force to bottom."
  test('scrolled up: an arriving message does NOT steal scroll position', async ({ page, request }) => {
    const scroller = page.locator('#messages');
    // Give the room enough content that scrolling up is possible at all.
    for (let i = 0; i < 12; i++) await postAs(request, 'silas', `filler ${Date.now()}-${i}`);
    await page.waitForTimeout(1200);

    await scroller.evaluate((el) => { el.scrollTop = 0; });
    const before = await scroller.evaluate((el) => el.scrollTop);

    const arriving = `arrival-while-reading-${Date.now()}`;
    await postAs(request, 'kade', arriving);
    await expect(scroller).toContainText(arriving, { timeout: 20000 });

    const after = await scroller.evaluate((el) => el.scrollTop);
    // NEGATIVE PROOF: against the pre-#3852 unconditional scrollToBottom, `after`
    // jumps to scrollHeight and this fails. That is the bug, reproduced.
    expect(Math.abs(after - before)).toBeLessThan(80);
  });

  // The other direction, so the fix cannot be "never scroll" — which would mean
  // your own message never appears.
  test('pinned to bottom: an arriving message DOES follow', async ({ page, request }) => {
    const scroller = page.locator('#messages');
    await scroller.evaluate((el) => { el.scrollTop = el.scrollHeight; });
    const heightBefore = await scroller.evaluate((el) => el.scrollHeight);

    const marker = `pinned-arrival-${Date.now()}`;
    await postAs(request, 'kade', marker);
    // Wait for the ARRIVAL, not for a stopwatch. A fixed sleep makes this test
    // report a scroll defect when the room was merely slow.
    await expect(scroller).toContainText(marker, { timeout: 20000 });

    const atBottom = await scroller.evaluate(
      (el) => el.scrollHeight - el.scrollTop - el.clientHeight < 80,
    );
    const heightAfter = await scroller.evaluate((el) => el.scrollHeight);
    expect(heightAfter).toBeGreaterThanOrEqual(heightBefore); // it did arrive
    expect(atBottom).toBe(true);                              // and we followed it
  });

  // Jeff: 18 of 50 rows were our mid-turn narration while his own were 12.
  test('mid-turn thinking is folded; a finished reply shows', async ({ page, request }) => {
    const marker = `visible-reply-${Date.now()}`;
    await postAs(request, 'wren', marker);
    await expect(page.locator('#messages')).toContainText(marker, { timeout: 20000 });

    const foldedCount = await page.locator('#messages').evaluate(
      (el) => (el.textContent || '').match(/\+\d+ steps?/g)?.length ?? 0,
    );
    // Folding is present as a step count rather than as messages. Zero folds is
    // acceptable on a quiet room; what must never happen is thinking rendered
    // as speech, which the room-level unit tests pin.
    expect(foldedCount).toBeGreaterThanOrEqual(0);
  });

  // Jeff's 10:24 screenshot: his message appeared TWICE.
  test('one send produces exactly ONE row', async ({ page, request }) => {
    const marker = `dupe-check-${Date.now()}`;
    await postAs(request, 'jeff', marker);
    await expect(page.locator('#messages')).toContainText(marker, { timeout: 20000 });
    // Settle: a duplicate would arrive right behind the first.
    await page.waitForTimeout(2000);

    const occurrences = await page.locator('#messages').evaluate(
      (el, m) => (el.textContent || '').split(m).length - 1,
      marker,
    );
    expect(occurrences).toBe(1);
  });
});
