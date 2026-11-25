// @test-type: e2e:ui — playwright: the Athena domain page's tables can be read on a phone
// @domain: tests
/**
 * #5034 AC1 — Jeff's phone, 2026-11-24 12:11: the Files, Code and API Contract lists on
 * /athena/domain.html?d=principles were cut to seven letters a row ("latform" forty times).
 * 54 policy checks said the domain was green. None of them looks at the page.
 *
 * What a reader needs at 390 px:
 *   1. the page itself never scrolls sideways;
 *   2. every table's own scroll box is as wide as the fold it sits in (a box squeezed
 *      beside a floated label is the cut Jeff saw);
 *   3. no cell hides its text behind a hover title: a phone has no hover.
 * The page's files are served from this tree (lib/athena-page-5034.cjs); data is live.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-readable-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { DOMAIN, PHONE, openDomainPage } = require('./lib/athena-page-5034.cjs');

test.describe(`#5034 the ${DOMAIN} domain page reads on a phone`, () => {
  test('the page body never scrolls sideways at 390 px', async ({ page }) => {
    const { errors } = await openDomainPage(page, PHONE);
    const over = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(over, `the page is ${over}px wider than the screen`).toBeLessThanOrEqual(1);
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('every table box is as wide as its fold, so its rows are not cut', async ({ page }) => {
    test.setTimeout(240_000);
    await openDomainPage(page, PHONE);
    // the Conformance fold draws its table only when the domain lint has answered (12 s alone,
    // over 60 s under load). Checked before that, its table is not there to measure: this test
    // passed for three hours while that table was squeezed to 105px (found 11-24 16:44).
    await page.waitForFunction(() => !/running the domain lint/.test(document.getElementById('conf-slot')?.textContent || ''), null, { timeout: 180_000 });
    await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
    const squeezed = await page.evaluate(() => [...document.querySelectorAll('details.fold')].flatMap((fold) => {
      const body = fold.querySelector(':scope > .body');
      if (!body) return [];
      const room = body.clientWidth - parseFloat(getComputedStyle(body).paddingLeft) - parseFloat(getComputedStyle(body).paddingRight);
      const name = (fold.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim();
      return [...body.querySelectorAll('.tbl, :scope > table')].filter((box) => box.clientWidth < room - 2)
        .map((box) => `${name}: table box ${Math.round(box.clientWidth)}px of ${Math.round(room)}px`);
    }));
    expect(squeezed, `tables squeezed narrower than their fold:\n${squeezed.join('\n')}`).toEqual([]);
  });

  test('no cell hides its text behind a hover title', async ({ page }) => {
    await openDomainPage(page, PHONE);
    const hidden = await page.evaluate(() => [...document.querySelectorAll('details.fold td [title]')]
      .filter((el) => el.getAttribute('title') && el.textContent.trim() !== el.getAttribute('title').trim())
      .map((el) => `${(el.closest('details')?.querySelector('summary')?.childNodes[0]?.textContent || '').trim()}: shows "${el.textContent.trim()}" for "${el.getAttribute('title')}"`));
    expect(hidden, `cells that show less than they hold:\n${hidden.join('\n')}`).toEqual([]);
  });

  test('NEGATIVE: the checks are red on a page built the old way', async ({ page }) => {
    // the two shapes Jeff's screenshots showed, drawn by hand so this proof does not depend on today's bug
    await page.setViewportSize(PHONE);
    await page.setContent(`<style>.src{float:right;width:240px}.tbl{overflow-x:auto;max-width:100%}.body{padding:0 16px}</style>
      <details class="fold" open><summary>Code (2) <span class="src">chorus:SourceFile · hasDomain = principles · graph</span></summary>
      <div class="body"><div class="tbl"><table><tr><td>platform/api/src/handlers/domain-conformance.ts</td></tr>
      <tr><td><span title="/loom/principles-list.html">…/principles-list.html</span></td></tr></table></div></div></details>`);
    const squeezed = await page.evaluate(() => {
      const body = document.querySelector('.body'); const box = document.querySelector('.tbl');
      return box.clientWidth < body.clientWidth - 32 - 2;
    });
    expect(squeezed, 'a table box beside a floated label must read as squeezed').toBe(true);
    const hidden = await page.evaluate(() => [...document.querySelectorAll('td [title]')].filter((el) => el.textContent.trim() !== el.getAttribute('title').trim()).length);
    expect(hidden, 'a hover-only path must be counted').toBe(1);
  });
});
