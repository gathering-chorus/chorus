// @test-type: e2e:ui — playwright browser flow (clearing-ui), live surface
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

// #4045 — no prod default. With CLEARING_URL unset this spec used to post into the
// LIVE Clearing on :3470 ("flow-probe <ts>", "dupe-check-<ts>") from every pipeline
// run — Jeff watched seven of them land in the room in one hour (2026-09-02, Kade).
// #3615 class: a test brings its own world or refuses. The variant has no Clearing
// yet, so unset = skip, loudly; set it to a variant room to run.
const CLEARING = process.env.CLEARING_URL;
test.skip(!CLEARING, 'CLEARING_URL unset — refusing to write into the live Clearing (#3615); point it at a variant room to run');

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
// #3966 hardened the room's write door: BRIDGE_TOKEN or CSS session, anonymous
// refused. The flow posts as a server-side caller, so it carries the same token
// the probe/responder/roles present — read from the file the server reads.
const fs = require('fs');
// CHORUS_HOME means the repo in shell env but ~/.chorus to the Clearing server —
// try both locations the server could have read its token from.
const BRIDGE_TOKEN = [
  `${process.env.CHORUS_HOME || ''}/bridge-auth-token`,
  `${process.env.HOME}/.chorus/bridge-auth-token`,
].map((p) => { try { return fs.readFileSync(p, 'utf8').trim(); } catch { return ''; } })
 .find(Boolean) || '';

async function postAs(request, from, text, type) {
  return request.post(`${CLEARING}/api/message`, {
    headers: { Authorization: `Bearer ${BRIDGE_TOKEN}` },
    data: type ? { from, text, type } : { from, text },
  });
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

  // #3862 — THE CANARY.
  //
  // Jeff, 2026-08-13: "i guess i get to be the canary in the coal mine." He is
  // right, and that is the defect this replaces. He reported missing messages
  // for three days; each time he was answered with a measurement instead of a
  // browser, and each time the answer was that it worked. It did not.
  //
  // What used to stand here was worse than nothing:
  //
  //     expect(foldedCount).toBeGreaterThanOrEqual(0);
  //
  // A count is never negative, so that assertion could not fail — written by me,
  // the same day, in a file whose header names the hollow-check shape. It passed
  // while three of my replies to Jeff were being hidden on his screen.
  //
  // These four shapes are the ones that were actually vanishing, taken from his
  // room's store: a reply naming another role, a reply from a turn that used
  // tools, a reply quoting a path, and a nudge. If any stops rendering, this
  // goes red — and the alarm is a test, not his evening.
  // The TYPE matters as much as the text. The first version of this block
  // posted all four as plain messages and passed against a build with the
  // pm-thinking rule reverted to hidden — it could not reach the rule it
  // claimed to guard. Proven by running it, in 23 seconds, against a reverted
  // instance on :3479. Every entry now carries the type the real emitter sends.
  const MUST_REACH_JEFF = [
    ['names another role', (m) => `${m} — Kade has streams, #3827 is mine.`, undefined],
    ['quotes a filesystem path', (m) => `${m} — the fix is in /Users/jeffbridwell/CascadeProjects/chorus/directing/clearing/public/index.html`, undefined],
    ['is a role-to-role nudge', (m) => `[nudge from silas | 2026-08-13 17:00 Boston] ${m} — the importmap fix is yours.`, undefined],
    ['says the word "blocked" in prose', (m) => `${m} — chorus-api event loop blocked 4712ms; not a real block.`, undefined],
    ['came from a turn that used tools', (m) => `${m} — Silas confirms his seam needs no change.`, 'pm-thinking'],
  ];

  for (const [shape, build, type] of MUST_REACH_JEFF) {
    test(`a reply that ${shape} reaches Jeff's room`, async ({ page, request }) => {
      const marker = `canary-${Date.now()}-${Math.floor(performance.now())}`;
      await postAs(request, 'wren', build(marker), type);
      // NEGATIVE PROOF: each shape was observed HIDDEN in the live room at 17:22
      // Boston. Reverting the matching rule in router.ts turns its case red —
      // the bug reproduced, not the fix asserted.
      await expect(page.locator('#messages')).toContainText(marker, { timeout: 20000 });
    });
  }

  // The other half. Without this, "make everything visible" passes the four
  // above and leaves Jeff a room full of machinery — which is the state his
  // phone was in this afternoon: e2e acks and tool errors as chat bubbles.
  test('machinery does NOT reach the room', async ({ page, request }) => {
    const marker = `machinery-${Date.now()}`;
    await postAs(request, 'silas', `[e2e-ack] silas received ${marker}`);
    const anchor = `anchor-${Date.now()}`;
    await postAs(request, 'jeff', anchor);

    // Wait on the anchor, so we are asserting absence at a moment the room has
    // demonstrably caught up — not absence because nothing has arrived yet.
    await expect(page.locator('#messages')).toContainText(anchor, { timeout: 20000 });
    await expect(page.locator('#messages')).not.toContainText(marker);
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
