// @test-type: e2e:ui — playwright: every fold on the Athena domain page is filled or says why it is empty
// @domain: tests
/**
 * #5034 AC4 — Jeff's phone, 2026-11-24: the Dependencies fold read "layer: none recorded"
 * while 54 policy checks said the domain was complete. The checks read the graph; this
 * reads the page as the browser drew it.
 *
 * A fold passes when it holds rows, or shows the Domain row's written reason ("Why empty:").
 * It fails when it is still loading, says it could not read or measure, or says nothing was
 * recorded. The words are read from the drawn page, never from the endpoints.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-folds-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { DOMAIN, openDomainPage } = require('./lib/athena-page-5034.cjs');

/**
 * Words a fold shows when it has no answer. Each names a state the reader cannot act on.
 * The loading states are matched whole: a bare "loading" would match a file or principle
 * whose name holds the word. "Not served" is not here: it is a true answer for a class no
 * door serves, and whether it is true is the true-data suite's question (Kade, 11-24).
 */
const NO_ANSWER = /loading the materialized view|loading live from the model|running the domain lint|none recorded|could not read|cannot reach|cannot measure|cannot read/i;

/** Run in the page: every fold that gives no answer, as "Fold name: the words it shows". */
function unansweredFolds(pattern) {
  const re = new RegExp(pattern, 'i');
  const nameOf = (fold) => (fold.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim().replace(/\s*\(.*$/, '');
  const out = [];
  for (const fold of document.querySelectorAll('details.fold, details.mfwrap')) {
    const body = fold.querySelector(':scope > .body, :scope > .mfbody');
    const text = (body?.innerText || '').replace(/\s+/g, ' ').trim();
    const hit = text.match(re);
    if (hit) { out.push(`${nameOf(fold)}: "${text.slice(Math.max(0, hit.index - 20), hit.index + 60).trim()}"`); continue; }
    // an empty fold must carry its written reason
    const empty = body && body.querySelector('.empty') && !body.querySelector('table, .mfgrid, .chip, p:not(.empty):not(.note):not(.src)');
    if (empty && !body.querySelector('.note')) out.push(`${nameOf(fold)}: empty, with no written reason ("${text.slice(0, 70)}")`);
  }
  return out;
}

test.describe(`#5034 every fold on the ${DOMAIN} domain page is filled or says why`, () => {
  // the conformance fold runs the whole domain lint on the server: 12 s alone, over 60 s
  // measured when four suites ran together (11-24 15:10). The wait below is the stated limit.
  test.setTimeout(240_000);

  test('no fold is loading, unread, unmeasured or unrecorded', async ({ page }) => {
    const { errors } = await openDomainPage(page);
    // the conformance fold runs the domain lint (about 12 s measured); wait for its verdict
    await page.waitForFunction(() => !/running the domain lint/.test(document.getElementById('conf-slot')?.textContent || ''), null, { timeout: 90_000 });
    await page.waitForSelector('#mfold-slot[data-mfold-ready="1"]', { timeout: 45_000 });
    const folds = await page.locator('details.fold, details.mfwrap').count();
    expect(folds, 'the page drew too few folds to be the domain page').toBeGreaterThan(10);
    const bad = await page.evaluate(unansweredFolds, NO_ANSWER.source);
    expect(bad, `folds that give the reader no answer:\n${bad.join('\n')}`).toEqual([]);
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('NEGATIVE: each no-answer state is named, and a filled fold and a reasoned empty fold are not', async ({ page }) => {
    await page.setContent(`
      <details class="fold" open><summary>Dependencies (0) <span class="src">x</span></summary><div class="body"><div><span class="lbl">layer</span> <span class="empty">none recorded</span></div></div></details>
      <details class="fold" open><summary>Conformance <span class="src">x</span></summary><div class="body"><p class="empty"><strong>Cannot measure</strong> — HTTP 500</p></div></details>
      <details class="fold" open><summary>Cards (?) <span class="src">could not read</span></summary><div class="body"><div class="bad">could not read Cards: HTTP 502</div></div></details>
      <details class="fold" open><summary>Tests (0) <span class="src">x</span></summary><div class="body"><p class="empty">loading the materialized view…</p></div></details>
      <details class="fold" open><summary>Alerts (0) <span class="src">x</span></summary><div class="body"><div class="empty">no alerts for this domain</div></div></details>
      <details class="fold" open><summary>Logs (0) <span class="src">x</span></summary><div class="body"><p class="note"><strong>Why empty:</strong> this domain runs no service.</p><div class="empty">no logs for this domain</div></div></details>
      <details class="fold" open><summary>Code (2) <span class="src">x</span></summary><div class="body"><div class="tbl"><table><tr><td>a.ts</td></tr></table></div></div></details>`);
    const bad = await page.evaluate(unansweredFolds, NO_ANSWER.source);
    expect(bad.map((b) => b.split(':')[0])).toEqual(['Dependencies', 'Conformance', 'Cards', 'Tests', 'Alerts']);
  });
});
