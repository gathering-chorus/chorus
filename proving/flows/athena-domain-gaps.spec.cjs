// @test-type: e2e:ui — playwright: each gap on the Athena domain page is a link to the card that will close it
// @domain: tests
/**
 * #5045 AC2 — Jeff's phone, 2026-11-25 15:47: "how do we resolve gaps". The Gaps fold showed a
 * sentence typed on the Domain row; nobody kept it true and it led nowhere. A gap is now the board
 * card that will close it: the row holds the card's number, and the fold shows that card, linked,
 * with the title, owner and standing the board gives it now.
 *
 * The fold reads GET /api/chorus/domain/<d>/gaps. That route is new with this card, so before the
 * land it is answered by THIS tree's handler (lib serveOwnGaps → platform/api/tests/lib/
 * own-routes-5044.ts), joined to the live board. The numbers a row holds are stated by the test:
 * the rows are written after the model deploy (platform/scripts/land-5045-rows).
 * The truth each test holds the page to is asked of the board directly (GET /api/chorus/card-story/<n>).
 *
 * Page code this suite opens and reads: platform/api/public/athena/domain.html.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-gaps.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, PHONE, openDomainPage, serveLintOnce, serveOwnRoutes, serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');

const BEFORE_5045 = '053b45343dd35620691020bca43d17629c46aa54';
const GAP = '5047';        // principles' walked gap: five value rows name removed principles
const DONE = '5044';       // a card the board holds as Done
const NO_CARD = '99999999'; // a number the board does not hold

/** The Gaps fold as drawn: { state, summary, text, rows: [{ id, state, link, text }] }, or null. */
const foldAsDrawn = (page) => page.evaluate(() => {
  const f = [...document.querySelectorAll('#main details.fold')].find((d) => /^\s*Gaps/.test(d.querySelector('summary')?.textContent || ''));
  if (!f) return null;
  const clean = (s) => String(s || '').replace(/\s+/g, ' ').trim();
  return { state: f.getAttribute('data-gaps'), summary: clean(f.querySelector('summary').textContent), text: clean(f.querySelector('.body').innerText),
    rows: [...f.querySelectorAll('[data-gap]')].map((r) => ({ id: r.getAttribute('data-gap'), state: r.getAttribute('data-state'),
      link: r.querySelector('a[href]') ? r.querySelector('a[href]').href : null, text: clean(r.innerText) })) };
});
const gapsDrawn = async (page) => { await page.waitForFunction(() => document.querySelector('#gaps-slot details'), null, { timeout: 45_000 }); return foldAsDrawn(page); };

/** The board's own word for a card: { title, owner, status }, or null when it holds none. */
async function boardCard(page, n) {
  const r = await page.request.get(`${BASE}/api/chorus/card-story/${n}`, { timeout: 20_000 });
  if (!r.ok()) return null;
  const c = await r.json();
  return c.title ? { title: c.title, owner: c.owner, status: c.status } : null;
}

test.describe('#5045 AC2 each gap is a link to the card that will close it', () => {
  test.setTimeout(240_000);

  test("a gap shows its card's number as a link, with the board's own title, owner and standing", async ({ page }) => {
    const truth = await boardCard(page, GAP);
    expect(truth, `the board no longer holds #${GAP}: pick another live card`).not.toBeNull();
    await openDomainPage(page, { gaps: { principles: GAP } });
    const fold = await gapsDrawn(page);
    expect(fold.rows.map((r) => r.id)).toEqual([GAP]);
    const row = fold.rows[0];
    expect(row.link, 'the gap is not a link').toMatch(new RegExp(`/${GAP}$`));
    expect(row.text).toContain(`#${GAP}`);
    expect(row.text).toContain(truth.title);
    expect(row.text).toContain(truth.owner);
    expect(row.text).toContain(truth.status);
    // the typed sentence is gone: nothing in the fold but the card
    expect(fold.summary).toContain('Gaps (1)');
    // where it sits, at phone width: the whole title is on the screen, wrapped, not cut or scrolled sideways
    const box = await page.evaluate((id) => { const r = document.querySelector(`[data-gap="${id}"]`); const b = r.getBoundingClientRect(); return { left: b.left, right: b.right, scroll: r.scrollWidth - r.clientWidth, vw: document.documentElement.clientWidth }; }, GAP);
    expect(box.left, 'the gap starts off the left edge').toBeGreaterThanOrEqual(0);
    expect(box.right, 'the gap runs off the right edge of a phone').toBeLessThanOrEqual(box.vw);
    expect(box.scroll, 'the gap scrolls sideways inside its fold').toBeLessThanOrEqual(0);
  });

  test('NEGATIVE: a gap whose card is Done is shown as closed, not open, and is not counted as an open gap', async ({ page }) => {
    const done = await boardCard(page, DONE);
    expect(done && done.status, `#${DONE} is no longer Done on the board: pick another closed card`).toBe('Done');
    await openDomainPage(page, { gaps: { principles: `${DONE},${GAP}` } });
    const fold = await gapsDrawn(page);
    expect(fold.rows.map((r) => [r.id, r.state]), 'the open gap first, then the closed one').toEqual([[GAP, 'open'], [DONE, 'closed']]);
    expect(fold.rows[1].text).toMatch(/· Done · closed$/);
    expect(fold.summary, 'only the open gap is counted').toContain('Gaps (1)');
    // and a domain whose only gap is closed counts none open
    await openDomainPage(page, { gaps: { principles: DONE } });
    const only = await gapsDrawn(page);
    expect(only.state).toBe('closed');
    expect(only.summary).toContain('Gaps (0)');
  });

  test('NEGATIVE: a number the board does not hold is said to be missing, and no title is made up', async ({ page }) => {
    expect(await boardCard(page, NO_CARD), 'the board holds the number this proof needs to be absent').toBeNull();
    await openDomainPage(page, { gaps: { principles: `${GAP},${NO_CARD}` } });
    const fold = await gapsDrawn(page);
    const missing = fold.rows.find((r) => r.id === NO_CARD);
    expect(missing, 'the missing card was dropped from the fold without a word').toBeTruthy();
    expect(missing.state).toBe('not on the board');
    expect(missing.text).toBe(`#${NO_CARD} not on the board`);
  });

  test('NEGATIVE: a domain whose row names no card says so; it does not borrow another domain\'s gap', async ({ page }) => {
    await openDomainPage(page, { domain: 'board', gaps: { board: '', principles: GAP } });
    const fold = await gapsDrawn(page);
    expect(fold.state).toBe('none');
    expect(fold.rows).toEqual([]);
    expect(fold.text).toBe('No open gap is recorded for board: its row names no card.');
  });

  test('NEGATIVE: a store that cannot be read is said so, never drawn as "no gap"', async ({ page }) => {
    await openDomainPage(page, { gaps: { principles: 'down' } });
    const fold = await gapsDrawn(page);
    expect(fold.state).toBe('unread');
    expect(fold.text).toContain('could not read Gaps');
    expect(fold.text).not.toContain('No open gap');
  });

  test('NEGATIVE: the page as it was before this card shows the typed sentence and no card link', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await serveLintOnce(page);
    await serveOwnRoutes(page, { gaps: { principles: GAP } });
    await serveAthenaFilesAsOf(page, BASE, BEFORE_5045);
    await page.goto(`${BASE}/athena/domain.html?d=principles`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#main .stats') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
    const fold = await foldAsDrawn(page);
    expect(fold, 'the old page drew no Gaps fold').not.toBeNull();
    expect(fold.rows, 'the old page drew a card row: this proof no longer shows the fault').toEqual([]);
    expect(fold.state, 'the old fold carries no state from the board').toBeNull();
  });
});
