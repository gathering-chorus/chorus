// @test-type: e2e:ui — playwright browser flow (clearing-no-leaked-markup-3868), live surface
/**
 * #3868 — no code commentary renders in Jeff's room.
 *
 * His phone, 2026-08-14 08:21: my #3865 comment about import maps was showing
 * as page copy below the message composer. `public/index.html` had a `-->`
 * closing the comment early, so the second paragraph fell outside it.
 *
 * #3865 shipped with three green flows. They proved the key generates and the
 * console is clean; not one of them looked at what the page says. The check and
 * the experience were not the same thing — again.
 *
 * NEGATIVE PROOF (#3734): asserts the RENDERED text, not the source. A source
 * grep for balanced comments passes on markup that renders wrong, which is the
 * exact hollow shape this file exists to avoid. Re-introducing the stray
 * terminator turns these red.
 */
const { test, expect } = require('@playwright/test');

// #4045 — no prod default. With CLEARING_URL unset this spec used to post into the
// LIVE Clearing on :3470 ("flow-probe <ts>", "dupe-check-<ts>") from every pipeline
// run — Jeff watched seven of them land in the room in one hour (2026-09-02, Kade).
// #3615 class: a test brings its own world or refuses. The variant has no Clearing
// yet, so unset = skip, loudly; set it to a variant room to run.
const CLEARING = process.env.CLEARING_URL;
test.skip(!CLEARING, 'CLEARING_URL unset — refusing to write into the live Clearing (#3615); point it at a variant room to run');

test.describe('#3868 — the room shows conversation, not source', () => {
  // Scoped OUTSIDE #messages deliberately. The first version of this test
  // asserted no '-->' anywhere on the page and failed — on a message where I
  // was telling Jeff about this very bug. A role discussing markup in the room
  // is conversation; markup escaping the page chrome is the defect. A check
  // that cannot separate those two is the shape this whole card is about.
  test('no comment terminator escapes into the page chrome', async ({ page }) => {
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 20000 });

    const chrome = await page.evaluate(() => {
      const clone = document.body.cloneNode(true);
      clone.querySelectorAll('#messages, .messages').forEach((n) => n.remove());
      return clone.innerText || '';
    });
    expect(chrome).not.toContain('-->');
  });

  test('no code commentary leaks into the rendered page', async ({ page }) => {
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 20000 });
    // Chrome only — same scoping as the test above, and for the same reason.
    // The first version of THIS test read the whole body and fired on a chat
    // message where I was describing the bug to Jeff. I fixed the sibling test
    // and left this one, so half the file could tell conversation from markup
    // and half could not. Caught by running it against live prod after the fix
    // had already landed and the card said Done.
    const body = await page.evaluate(() => {
      const clone = document.body.cloneNode(true);
      clone.querySelectorAll('#messages, .messages').forEach((n) => n.remove());
      return clone.innerText || '';
    });

    // Phrases from the leaked block, verbatim off Jeff's screenshot. Named
    // rather than pattern-matched: a broad "looks like code" check would fire
    // on a role legitimately discussing an import map in the room.
    for (const leaked of [
      'NOT redundant with the prefix',
      "imports '@noble/hashes/utils' with NO extension",
      'Exact keys win over prefix keys',
    ]) {
      expect(body, `leaked comment text on the page: ${leaked}`).not.toContain(leaked);
    }
  });

  test('the page still renders its real content — the check is not passing on an empty page', async ({ page }) => {
    // Without this, "body contains no comment text" would pass on a blank
    // page, which is the state the room was in for 30s yesterday.
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#messages', { timeout: 20000 });
    await expect(page.locator('body')).toContainText('Clearing');
  });
});
