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
const { BASE, DOMAIN, openDomainPage, openModelView, doorTruth, serveOwnAthenaFiles, serveDiscoveryAsLanded } = require('./lib/athena-page-5034.cjs');

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

  // #5040 — the link opens the class's rows as a TABLE (rows.html?kind=<Class>) and names the
  // collection beside it. It linked the collection itself, which on Jeff's phone opened a screen
  // of raw data (2026-11-25 08:14). That the rows page then lists every row is the next test.
  test('the ops view links each served class to its rows page and names the collection that holds them', async ({ page }) => {
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
      if (t.collection && s.href !== `${BASE}/athena/rows.html?kind=${encodeURIComponent(t.kind)}`) wrong.push(`${t.kind}: links to ${s.href || 'nothing'} (${s.text}), its rows page is /athena/rows.html?kind=${t.kind}`);
      if (t.collection && !s.text.includes(t.collection)) wrong.push(`${t.kind}: the fold does not name its collection ${t.collection} (it says: ${s.text})`);
      if (t.held > 0 && !s.href) wrong.push(`${t.kind}: the model holds ${t.held} rows, yet the page links to nothing (${s.text})`);
      if (!t.collection && s.href) wrong.push(`${t.kind}: no door lists it, yet the page links to ${s.href}`);
    }
    expect(wrong, `the Vocabulary fold disagrees with the doors:\n${wrong.join('\n')}`).toEqual([]);
  });

  test('the rows page lists every row of a served class (platform/api/public/athena/rows.js)', async ({ page }) => {
    // rows.html?kind=<Class> is where the Vocabulary link's class can be read as a table. It
    // dropped the /v1 from the collection discovery names, which a generated API does not answer.
    const served = (await doorTruth(page)).filter((t) => t.held > 0);
    expect(served.length, 'the model holds no rows for any class of this domain').toBeGreaterThan(0);
    await serveOwnAthenaFiles(page);
    await serveDiscoveryAsLanded(page);
    for (const t of served) {
      await page.goto(`${BASE}/athena/rows.html?kind=${encodeURIComponent(t.kind)}`, { waitUntil: 'domcontentloaded' });
      // the page opens on a "loading live from the model…" placeholder (itself class "empty"): wait for it to go
      await page.waitForFunction(() => { const m = document.getElementById('main'); return m && !/loading live from the model/.test(m.textContent); }, null, { timeout: 45_000 });
      const drawn = await page.evaluate(() => ({ rows: document.querySelectorAll('#main table.rows tbody tr').length, says: document.querySelector('#main .err, #main .empty')?.textContent.trim() || null }));
      expect(drawn, `rows.html?kind=${t.kind}: the model holds ${t.held} rows`).toEqual({ rows: t.held, says: null });
    }
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

// =============================================================================================
// #5041 AC2 — the page says each thing once and counts it truly.
//
// Jeff's phone, 2026-11-25: "files seems same as code fold?", "are code and tests counts really
// right? seem big?". Two folds read the same 50 CodeFile rows: "Files" listed them all, "Code"
// listed 40 and said "and 10 more". And: "i dont want to hide status and pipeline" (10:50).
//
// Read from the drawn page; the truth is the store's own count per kind, asked directly:
//   - the Code fold lists every file once, grouped by kind, each count equal to the store's;
//   - exactly one fold lists the domain's files (a fold whose rows are the store's CodeFile paths
//     of more than one kind; the Tests fold lists tests only and is a different thing);
//   - the Status tile and the Pipeline fold are drawn, as they were;
//   - a files read that fails draws "could not read", never an empty Code fold;
//   - the Description fold is drawn only when it says something the purpose line does not.
// NEGATIVE: the page Jeff walked (cut from git) is red on its two folds; a page drawn from a
// files answer one row short is red on the kind count; a page with the Pipeline fold or the
// Status tile taken out is red.
//
// #4235's static test (platform/api/tests/domain-page-files-4235.test.ts) is retired with
// renderFiles, the function it read as text. What it held, "a failed read shows an unknown count,
// never zero files", is held here as drawn: "a files read that fails says so".
//
// Page code read as drawn: platform/api/public/athena/domain.html (renderCode, the tiles,
// the Description fold), platform/api/public/athena/facets.js (the Pipeline fold).
// =============================================================================================
const { openOldDomainPage, PHONE } = require('./lib/athena-page-5034.cjs');

/** The store's own count of this domain's CodeFile rows per kind, and their paths per kind. */
async function storeFilesByKind(request) {
  const r = await request.post(`${BASE}/sparql-read`, { timeout: 30_000, headers: { Accept: 'application/sparql-results+json' },
    data: { query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?k (COUNT(DISTINCT ?f) AS ?n) (GROUP_CONCAT(DISTINCT ?path; separator="\\n") AS ?paths) WHERE { GRAPH ?g { ?f a chorus:CodeFile ; chorus:hasDomain <https://jeffbridwell.com/chorus#${DOMAIN}> ; chorus:filePath ?path . OPTIONAL { ?f chorus:hasKind ?k } } } GROUP BY ?k` } });
  if (!r.ok()) throw new Error(`the store did not answer for the files of ${DOMAIN}: HTTP ${r.status()}`);
  const out = {};
  for (const b of (await r.json()).results.bindings) out[b.k ? String(b.k.value).split('#').pop().replace(/^code-kind-/, '') : 'no kind'] = { n: Number(b.n.value), paths: String(b.paths.value).split('\n') };
  return out;
}

/** Run in the page: what each fold lists, what the Code fold says, the Status tile and the Pipeline fold. */
function drawnOnce() {
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const nameOf = (f) => (f.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim().replace(/\s*\(.*$/, '');
  const folds = [...document.querySelectorAll('details.fold')].map((f) => ({ name: nameOf(f), summary: text(f.querySelector(':scope > summary')), cells: [...f.querySelectorAll('td')].map(text), rows: f.querySelectorAll('tr td:first-child').length, body: text(f.querySelector(':scope > .body')) }));
  const code = [...document.querySelectorAll('details.fold')].find((f) => nameOf(f) === 'Code');
  // each group: the kind its heading names, and the paths listed in the table right under it
  const groups = code ? [...code.querySelectorAll('h3[data-kind]')].map((h) => ({ kind: h.dataset.kind, heading: text(h), paths: [...(h.nextElementSibling?.querySelectorAll('td') || [])].map(text) })) : [];
  const status = [...document.querySelectorAll('.stats .stat')].find((s) => text(s.querySelector('.l')) === 'Status');
  return { folds, groups, codeSummary: code ? text(code.querySelector(':scope > summary')) : null, status: status ? text(status.querySelector('.v')) : null };
}

/** Every place the drawn page says a thing twice, miscounts it, or has lost Status or Pipeline. */
function onceFaults(drawn, store) {
  const out = [];
  const kindOf = new Map(Object.entries(store).flatMap(([k, v]) => v.paths.map((p) => [p, k])));
  const total = Object.values(store).reduce((a, v) => a + v.n, 0);
  // a fold lists the domain's files when its cells are the store's CodeFile paths of more than one kind
  const fileLists = drawn.folds.filter((f) => new Set(f.cells.filter((c) => kindOf.has(c)).map((c) => kindOf.get(c))).size > 1).map((f) => f.name);
  if (fileLists.length !== 1 || fileLists[0] !== 'Code') out.push(`the domain's files are listed by ${fileLists.length} folds (${fileLists.join(', ') || 'none'}): there must be one, Code`);
  if (drawn.codeSummary === null) out.push('the Code fold is not drawn');
  else {
    if (/could not read|\(\?\)/.test(drawn.codeSummary + ' ' + (drawn.folds.find((f) => f.name === 'Code')?.body || ''))) out.push(`Code: the fold could not read its files (${drawn.codeSummary.slice(0, 60)})`);
    if (!drawn.codeSummary.startsWith(`Code (${total})`)) out.push(`Code: the fold's total reads "${drawn.codeSummary.slice(0, 12)}", the store holds ${total}`);
    for (const [k, v] of Object.entries(store)) {
      const g = drawn.groups.filter((x) => x.kind === k);
      const listed = g.reduce((a, x) => a + x.paths.length, 0);
      if (g.length !== 1) out.push(`Code: ${g.length} groups for kind "${k}", the store holds ${v.n} such files`);
      else if (listed !== v.n) out.push(`Code: ${listed} ${k} listed, the store holds ${v.n}`);
      else if (JSON.stringify([...g[0].paths].sort()) !== JSON.stringify([...v.paths].sort())) out.push(`Code: the ${k} files listed are not the ones the store holds`);
      // the count the reader sees, in the group's heading and in the fold's summary line
      if (g.length === 1 && !new RegExp(`\\b${v.n} file`).test(g[0].heading)) out.push(`Code: the "${k}" heading reads "${g[0].heading}", the store holds ${v.n}`);
      if (!new RegExp(`(^|[ ·])${v.n} ${k}( ·| chorus:|$)`).test(drawn.codeSummary)) out.push(`Code: the summary does not say "${v.n} ${k}" (it says: ${drawn.codeSummary.slice(0, 90)})`);
    }
    for (const g of drawn.groups) if (!store[g.kind]) out.push(`Code: a group for kind "${g.kind}", which the store does not hold for this domain`);
    const all = drawn.groups.flatMap((g) => g.paths);
    if (new Set(all).size !== all.length) out.push(`Code: ${all.length - new Set(all).size} file(s) listed more than once`);
  }
  if (!drawn.status) out.push('the Status tile is not drawn');
  const pipeline = drawn.folds.find((f) => f.name === 'Pipeline');
  if (!pipeline) out.push('the Pipeline fold is not drawn');
  else if (!pipeline.rows) out.push('the Pipeline fold is drawn with no stage in it');
  return out;
}

test.describe(`#5041 the ${DOMAIN} ops view says each thing once and counts it truly`, () => {
  test.setTimeout(240_000);

  test('the files are listed once, in the Code fold, by kind, each count the store\'s own; Status and Pipeline are drawn', async ({ page, request }) => {
    const store = await storeFilesByKind(request);
    expect(Object.keys(store).length, `the store holds no file for ${DOMAIN}: nothing to check`).toBeGreaterThan(1);
    const { errors } = await openDomainPage(page);
    const faults = onceFaults(await page.evaluate(drawnOnce), store);
    expect(faults, `the page says a thing twice or miscounts it:\n${faults.join('\n')}`).toEqual([]);
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('a files read that fails says so: "could not read", an unknown count, never an empty Code fold', async ({ page }) => {
    await page.route((url) => url.pathname === '/owl/code/files', (route) => route.fulfill({ status: 500, contentType: 'application/json', body: '{"error":"planted by the test"}' }));
    await openDomainPage(page);
    const code = (await page.evaluate(drawnOnce)).folds.find((f) => f.name === 'Code');
    expect(code, 'the Code fold must still be drawn when its read fails').toBeTruthy();
    expect(code.summary, 'an unknown count, never a zero').toMatch(/^Code \(\?\)/);
    expect(code.body).toMatch(/could not read Code: 500/);
    expect(code.rows, 'no file may be listed from a read that failed').toBe(0);
    // and it starts open, so the failure is seen (a healthy Code fold starts closed)
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#code-slot details');
    expect(await page.evaluate(() => document.querySelector('#code-slot details').open)).toBe(true);
  });

  test('the Description fold is drawn only when it says something the purpose does not', async ({ page }) => {
    // the Domain row as the door serves it, with its comment replaced: once by the purpose in other
    // capitals and spacing (a repeat), once by the purpose and more (not a repeat)
    let comment = (purpose) => `  ${purpose.toUpperCase().replace(/ /g, '   ')}\n`;
    await page.route((url) => url.pathname === `/owl/domains/${DOMAIN}`, async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      const data = Array.isArray(body.data) ? body.data[0] : body.data;
      expect(data.purpose, 'the Domain row has no purpose: this proof needs one').toBeTruthy();
      data.comment = comment(data.purpose);
      return route.fulfill({ response: live, json: body });
    });
    const names = () => page.evaluate(() => [...document.querySelectorAll('details.fold > summary')].map((s) => (s.childNodes[0]?.textContent || '').trim()));
    await openDomainPage(page);
    expect(await names(), 'a Description that only repeats the purpose is not drawn').not.toContain('Description');
    comment = (purpose) => `${purpose} And one thing more.`;
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#code-slot details');
    expect(await names(), 'a Description that says more than the purpose is drawn').toContain('Description');
    expect(await page.evaluate(() => [...document.querySelectorAll('details.fold')].find((f) => /^Description/.test(f.querySelector('summary').textContent)).querySelector('.body').textContent)).toMatch(/And one thing more\./);
  });

  test('NEGATIVE: the page Jeff walked is red on its two folds over the same files', async ({ page, request }) => {
    const store = await storeFilesByKind(request);
    await page.setViewportSize(PHONE);
    await openOldDomainPage(page, BASE);
    await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
    const faults = onceFaults(await page.evaluate(drawnOnce), store).join('\n');
    expect(faults).toMatch(/^the domain's files are listed by 2 folds \((Code, Files|Files, Code)\)/m);
    expect(faults, 'the old Code fold gave no count per kind').toMatch(/^Code: 0 groups for kind "test"/m);
  });

  test('NEGATIVE: a kind count that differs from the store is red, and so is a page without Pipeline or Status', async ({ page, request }) => {
    const store = await storeFilesByKind(request);
    const kind = Object.keys(store).find((k) => k !== 'no kind' && store[k].n > 1);
    expect(kind, 'this proof needs a kind with two files or more').toBeTruthy();
    // the door's own answer, one file of that kind short: the page draws what it was given
    await page.route((url) => url.pathname === '/owl/code/files', async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      const i = body.data.findIndex((f) => String(f.hasKind).endsWith(kind));
      if (i >= 0) body.data.splice(i, 1);
      return route.fulfill({ response: live, json: body });
    });
    await openDomainPage(page);
    const drawn = await page.evaluate(drawnOnce);
    const faults = onceFaults(drawn, store).join('\n');
    expect(faults).toMatch(new RegExp(`^Code: ${store[kind].n - 1} ${kind} listed, the store holds ${store[kind].n}$`, 'm'));
    expect(faults, 'the total is one short as well').toMatch(/^Code: the fold's total reads/m);
    // Jeff, 10:50: Status and Pipeline stay. Taken out of the drawn page, each is named.
    await page.evaluate(() => {
      [...document.querySelectorAll('details.fold')].find((f) => /^Pipeline/.test(f.querySelector('summary').textContent)).remove();
      [...document.querySelectorAll('.stats .stat')].find((s) => s.querySelector('.l').textContent.trim() === 'Status').remove();
    });
    const gone = onceFaults(await page.evaluate(drawnOnce), store);
    expect(gone).toContain('the Pipeline fold is not drawn');
    expect(gone).toContain('the Status tile is not drawn');
  });
});
