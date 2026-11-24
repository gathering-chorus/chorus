// @test-type: e2e:ui — playwright: the Athena domain pages say nothing false about what is served
// @domain: tests
/**
 * #5034 AC3 — Jeff's phone, 2026-11-24: the model view said "Principle — Not served… it has
 * no shape" while the door held 14 principles. The page was not broken; it was wrong.
 *
 * The truth is asked of the doors directly (lib doorTruth): for each class the domain declares,
 * which collection serves it and which rows it holds. Then the drawn page is read:
 *   - a class with rows shows one card per row, the same names, and never "Not served";
 *   - "Not served" appears only for a class no door lists;
 *   - the ops view's Vocabulary row links to the collection that holds those rows.
 *
 * Page code this suite reads the drawn answer of, named so a change to any of it runs this suite:
 *   platform/api/public/athena/model-fold.js (served / not served, one card per row),
 *   platform/api/public/athena/domain.html (the Vocabulary fold).
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-true-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, DOMAIN, openDomainPage, openModelView, doorTruth } = require('./lib/athena-page-5034.cjs');

/** Run in the page: what the model fold shows for each class, { kind: { cards: [names], says: 'note text'|null } }. */
function drawnClasses() {
  const out = {};
  for (const h of document.querySelectorAll('[data-mf="instances"] > h3')) {
    const next = h.nextElementSibling;
    out[h.textContent.trim()] = {
      cards: next?.classList.contains('mfgrid') ? [...next.querySelectorAll('a.mfcard .mfm')].map((n) => n.textContent.trim()).sort() : [],
      says: next && !next.classList.contains('mfgrid') ? next.textContent.trim().slice(0, 60) : null,
    };
  }
  return out;
}

/** Every place the drawn page disagrees with the doors, as plain lines. */
function lies(truth, drawn) {
  const out = [];
  for (const t of truth) {
    const d = drawn[t.kind];
    if (!d) { out.push(`${t.kind}: declared by the domain, missing from the page`); continue; }
    const notServed = /not served/i.test(d.says || '');
    if (t.held > 0 && notServed) out.push(`${t.kind}: the page says "Not served", the model holds ${t.held} rows`);
    else if (t.held > 0 && d.cards.length !== t.held) out.push(`${t.kind}: the page shows ${d.cards.length} cards, the model holds ${t.held} rows (${d.says || 'no note'})`);
    else if (t.rows && JSON.stringify(d.cards) !== JSON.stringify(t.rows)) out.push(`${t.kind}: the cards are not the rows the door answers with`);
    if (t.held === 0 && !t.collection && !notServed) out.push(`${t.kind}: nothing holds or serves it, yet the page does not say "Not served" (it says: ${d.says || d.cards.length + ' cards'})`);
  }
  return out;
}

test.describe(`#5034 the ${DOMAIN} domain pages say only what the doors hold`, () => {
  test('the model view shows each served class with a card for every row the door holds', async ({ page }) => {
    const truth = await doorTruth(page);
    expect(truth.length, `the model declares no vocabulary for ${DOMAIN}; nothing to check`).toBeGreaterThan(0);
    await openModelView(page);
    const wrong = lies(truth, await page.evaluate(drawnClasses));
    expect(wrong, `the model view disagrees with the doors:\n${wrong.join('\n')}`).toEqual([]);
  });

  test('the ops view links each served class to the collection that holds its rows', async ({ page }) => {
    const truth = await doorTruth(page);
    await openDomainPage(page);
    const shown = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll('details.fold')]
      .filter((f) => /^Vocabulary/.test(f.querySelector('summary')?.textContent || ''))
      .flatMap((f) => [...f.querySelectorAll('tr')].filter((tr) => tr.querySelector('td'))
        .map((tr) => [tr.cells[0].textContent.trim(), { href: tr.querySelector('a')?.href || null, text: tr.cells[1].textContent.trim() }]))));
    const wrong = [];
    for (const t of truth) {
      const s = shown[t.kind];
      if (!s) { wrong.push(`${t.kind}: not in the Vocabulary fold`); continue; }
      if (t.collection && s.href !== `${BASE}/owl${t.collection}`) wrong.push(`${t.kind}: links to ${s.href || 'nothing'} (${s.text}), the door serves it at /owl${t.collection}`);
      if (t.held > 0 && !s.href) wrong.push(`${t.kind}: the model holds ${t.held} rows, yet the page links to nothing (${s.text})`);
      if (!t.collection && s.href) wrong.push(`${t.kind}: no door lists it, yet the page links to ${s.href}`);
    }
    expect(wrong, `the Vocabulary fold disagrees with the doors:\n${wrong.join('\n')}`).toEqual([]);
  });

  test('NEGATIVE: drawn as before the fix, the page is caught saying "Not served" over rows the door holds', async ({ page }) => {
    const truth = await doorTruth(page);
    const served = truth.filter((t) => t.held > 0);
    expect(served.length, 'the model holds no rows for any class of this domain: this proof needs one').toBeGreaterThan(0);
    // the directory with the served classes left out: the model view of 2026-11-24 12:10
    await openModelView(page, { omit: served.map((t) => t.kind) });
    const wrong = lies(truth, await page.evaluate(drawnClasses));
    expect(wrong.length, `every served class must be named, got:\n${wrong.join('\n')}`).toBe(served.length);
    for (const w of wrong) expect(w).toMatch(/says "Not served", the model holds \d+ rows/);
    // and the other direction: a class nothing holds or serves must read "Not served"; a page that showed cards for it is named
    const none = [{ kind: 'Ghost5034', held: 0, collection: null, rows: null }];
    expect(lies(none, { Ghost5034: { cards: ['a'], says: null } })).toHaveLength(1);
    expect(lies(none, { Ghost5034: { cards: [], says: 'Not served. Declared as vocabulary' } })).toEqual([]);
  });
});
