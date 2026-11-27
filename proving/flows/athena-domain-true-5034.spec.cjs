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
  // (#5044: a table wider than the phone has one line above it saying it scrolls; the table is the next thing down)
  const tableUnder = (h) => { const n = h.nextElementSibling; return n && n.classList.contains('cut-cue') ? n.nextElementSibling : n; };
  const groups = code ? [...code.querySelectorAll('h3[data-kind]')].map((h) => ({ kind: h.dataset.kind, heading: text(h), paths: [...(tableUnder(h)?.querySelectorAll('td') || [])].map(text) })) : [];
  // #5045 — the tile no longer shows a typed status: it shows the API version in production
  // (data-api). The page Jeff walked before that card labelled it "Status"; both are the same tile.
  const status = [...document.querySelectorAll('.stats .stat')].find((s) => s.hasAttribute('data-api') || text(s.querySelector('.l')) === 'Status');
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
    // a domain with no description at all draws no Description fold (it drew an open, empty one:
    // Borg Queen, 11-25 12:34). The page falls back to the collection row's comment, so both are blanked.
    comment = () => '';
    await page.route((url) => url.pathname === '/owl/domains', async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      for (const row of body.data || []) if (row.name === DOMAIN) delete row.comment;
      return route.fulfill({ response: live, json: body });
    });
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#code-slot details');
    expect(await names(), 'a domain with no description draws no Description fold').not.toContain('Description');
    expect((await names()).length, 'and the rest of the page is still drawn').toBeGreaterThan(10);
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
      document.querySelector('.stats .stat[data-api]').remove();
    });
    const gone = onceFaults(await page.evaluate(drawnOnce), store);
    expect(gone).toContain('the Pipeline fold is not drawn');
    expect(gone).toContain('the Status tile is not drawn');
  });
});

// =============================================================================================
// #5044 AC2 — the Pipeline fold agrees with the folds beside it.
//
// Jeff's phone, 2026-11-25 15:45 and 15:51: "pipeline section does not make sense to me seems like
// we need to analyze and fix". The fold read shape "No cards" beside Cards (16) and ship "Nothing
// shipped" above Releases (5). Measured: three of its rows read an old list of 25 domain names
// (principles is not on it; the 404 was turned into "nothing"), ship counted Done cards in a list
// with the Done cards taken out (it could not move for any domain), and build read links nothing
// writes any more (0 files beside a Code fold of 50).
//
// Each row now counts what the fold beside it lists. Read from the drawn page, every count taken
// from the OTHER fold's own heading, never from the Pipeline's read:
//   shape   the number in Cards (n)          ship    the number in Releases (n)
//   build   Code (n), Tests (n), API Contract (n)      prove   Alerts (n)
//   design  a percentage, with how many shape fields are present and missing
// and every status is plain words (not started, in progress, complete, could not read).
// Before the land the fold's answer is this tree's handler on the live server's reads (lib
// serveOwnRoutes, platform/api/tests/lib/own-routes-5044.ts; the handler is
// platform/api/src/handlers/chorus-domain-pipeline.ts). After it, the server's own.
// NEGATIVE: the answer main gave for principles on 2026-11-25 (kept below, as served) is red on
// shape, build and ship; a sibling read that fails draws "could not read", and a fold that drew
// it as "not started" is named.
// Named limit: what the five steps MEAN (when a domain is "complete" at a step) is not settled
// here; the thresholds are the ones the fold had. Jeff: Status and Pipeline "need analysis".
// =============================================================================================
const { ownPipeline } = require('./lib/athena-page-5034.cjs');

// GET /api/chorus/domain/principles/pipeline as main served it, 2026-11-25 18:05, while the same
// server's /cards listed 16 cards and /releases 5
const PIPELINE_ON_MAIN = { data: { domain: 'principles', stages: [
  { name: 'shape', status: 'not_started', evidence: 0, summary: 'No cards' },
  { name: 'design', status: 'not_started', evidence: 0, summary: 'Not started' },
  { name: 'build', status: 'complete', evidence: 26, summary: '0 source, 26 tests, 0 endpoints' },
  { name: 'prove', status: 'complete', evidence: 3, summary: '3 alert rules' },
  { name: 'ship', status: 'not_started', evidence: 0, summary: 'Nothing shipped' },
] } };

/** Run in the page: the Pipeline fold's rows, and the number each other fold states in its own heading. */
function drawnPipeline() {
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const fold = (name) => [...document.querySelectorAll('details.fold')].find((f) => (f.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim().replace(/\s*\(.*$/, '') === name);
  // "Cards (16)", "Tests (26 files · 175 cases)", "Code (50) 12 code · …": the first number in the brackets
  const stated = (name) => { const m = /\((\d+)/.exec(text(fold(name)?.querySelector(':scope > summary'))); return m ? Number(m[1]) : null; };
  const p = fold('Pipeline');
  return {
    rows: p ? [...p.querySelectorAll('tr')].filter((tr) => tr.cells[0]?.tagName === 'TD').map((tr) => ({ name: text(tr.cells[0]), status: text(tr.cells[1]), summary: text(tr.cells[2]) })) : null,
    folds: { Cards: stated('Cards'), Releases: stated('Releases'), Code: stated('Code'), Tests: stated('Tests'), 'API Contract': stated('API Contract'), Alerts: stated('Alerts') },
  };
}

/** Every place the Pipeline fold disagrees with a fold beside it, or says a status that is not plain words. */
function pipelineFaults({ rows, folds }) {
  if (!rows) return ['the Pipeline fold is not drawn'];
  const out = [];
  const row = Object.fromEntries(rows.map((r) => [r.name, r]));
  for (const n of ['shape', 'design', 'build', 'prove', 'ship']) if (!row[n]) out.push(`${n}: the row is not drawn`);
  for (const [name, n] of Object.entries(folds)) if (n === null) out.push(`the ${name} fold states no count to hold the Pipeline to`);
  for (const r of rows) if (!['not started', 'in progress', 'complete', 'could not read'].includes(r.status)) out.push(`${r.name}: the status "${r.status}" is not plain words`);
  const unread = (r) => r && r.status === 'could not read';
  const firstNumber = (s) => { const m = /^(\d+)/.exec(s); return m ? Number(m[1]) : 0; };
  const hold = (r, said, fold, n) => { if (r && !unread(r) && n !== null && said !== n) out.push(`${r.name}: says ${said} (${r.summary}), the ${fold} fold lists ${n}`); };
  if (row.shape) hold(row.shape, firstNumber(row.shape.summary), 'Cards', folds.Cards);
  if (row.ship) hold(row.ship, firstNumber(row.ship.summary), 'Releases', folds.Releases);
  if (row.prove) hold(row.prove, firstNumber(row.prove.summary), 'Alerts', folds.Alerts);
  if (row.build && !unread(row.build)) {
    const m = /^(\d+) files, (\d+) test files, (\d+) routes$/.exec(row.build.summary) || (row.build.summary === 'No code' ? [0, 0, 0, 0] : null);
    if (!m) out.push(`build: "${row.build.summary}" does not state its files, test files and routes`);
    else { hold(row.build, Number(m[1]), 'Code', folds.Code); hold(row.build, Number(m[2]), 'Tests', folds.Tests); hold(row.build, Number(m[3]), 'API Contract', folds['API Contract']); }
  }
  if (row.design && !unread(row.design) && !/^\d+%: \d+ present, \d+ missing$/.test(row.design.summary)) out.push(`design: "${row.design.summary}" does not state its percentage and fields`);
  // a row that says it has not started beside a fold that lists rows is the fault Jeff photographed
  for (const [n, fold] of [['shape', 'Cards'], ['ship', 'Releases'], ['prove', 'Alerts']]) if (row[n] && row[n].status === 'not started' && folds[fold] > 0) out.push(`${n}: "not started" beside ${fold} (${folds[fold]})`);
  return out;
}

test.describe(`#5044 the Pipeline fold of ${DOMAIN} agrees with the folds beside it`, () => {
  test.setTimeout(300_000);
  const isPipelineRead = (url) => /\/api\/chorus\/domain\/[^/]+\/pipeline$/.test(url.pathname);
  const redraw = async (page) => {
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => [...document.querySelectorAll('#facets details.fold summary')].some((s) => /^Pipeline/.test(s.textContent)) && document.querySelector('#code-slot details'), null, { timeout: 120_000 });
  };

  test('each Pipeline row counts what the fold beside it lists, in plain words', async ({ page }) => {
    const { errors } = await openDomainPage(page);
    const drawn = await page.evaluate(drawnPipeline);
    expect(drawn.folds.Cards, `the Cards fold lists cards for ${DOMAIN}: this check needs some (drawn: ${JSON.stringify(drawn)})`).toBeGreaterThan(0);
    expect(drawn.folds.Releases, 'and the Releases fold lists releases').toBeGreaterThan(0);
    const faults = pipelineFaults(drawn);
    expect(faults, `the Pipeline fold disagrees with the page around it:\n${faults.join('\n')}\n(rows: ${JSON.stringify(drawn.rows)})`).toEqual([]);
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('NEGATIVE: the answer main gave on 2026-11-25 is red on shape, build and ship', async ({ page }) => {
    await openDomainPage(page);
    await page.route(isPipelineRead, (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(PIPELINE_ON_MAIN) }));
    await redraw(page);
    const faults = pipelineFaults(await page.evaluate(drawnPipeline)).join('\n');
    expect(faults).toMatch(/^shape: says 0 \(No cards\), the Cards fold lists \d+$/m);
    expect(faults).toMatch(/^ship: says 0 \(Nothing shipped\), the Releases fold lists \d+$/m);
    expect(faults).toMatch(/^build: "0 source, 26 tests, 0 endpoints" does not state its files, test files and routes$/m);
    expect(faults, 'and its statuses were machine words').toMatch(/^shape: the status "not_started" is not plain words$/m);
    expect(faults, 'prove was right on main, and is not named').not.toMatch(/^prove: says/m);
  });

  test('NEGATIVE: a read that fails is drawn "could not read", and the same row drawn as "not started" is named', async ({ page }) => {
    await openDomainPage(page);
    // this tree's handler, with its releases read made to fail
    const failed = await ownPipeline(DOMAIN, '/releases');
    await page.route(isPipelineRead, (route) => route.fulfill({ status: failed.status, contentType: 'application/json', body: failed.body }));
    await redraw(page);
    const drawn = await page.evaluate(drawnPipeline);
    const ship = drawn.rows.find((r) => r.name === 'ship');
    expect(ship, `the ship row (rows: ${JSON.stringify(drawn.rows)})`).toMatchObject({ status: 'could not read', summary: 'could not read releases' });
    expect(drawn.rows.find((r) => r.name === 'shape').status, 'the rows whose reads answered are untouched').not.toBe('could not read');
    expect(pipelineFaults(drawn), 'a row that says it could not read is not a contradiction').toEqual([]);
    // what the old handler did with the same failure: "not started", beside the releases the page lists
    const old = { ...drawn, rows: drawn.rows.map((r) => (r.name === 'ship' ? { name: 'ship', status: 'not started', summary: 'No releases' } : r)) };
    expect(pipelineFaults(old).join('\n')).toMatch(/^ship: "not started" beside Releases \(\d+\)$/m);
  });
});

// =============================================================================================
// #5045 AC1 — the tile shows the API version in production, never a typed status.
// In this file, in its own block, and not a file of its own: the layout rule counts every file
// under proving/flows (arch-domain-directory-layout; four new files took it from 1232 to 1236 on
// cicd run 4 of #5045). The block has its own names; nothing above is changed by it.
// =============================================================================================
{
/**
 * #5045 AC1 — Jeff's phone, 2026-11-25 15:47: "we did not analyze status yet". The Status tile
 * showed a word typed on the Domain row ("operating"), never checked against anything. Jeff,
 * 2026-11-26 11:08: a domain has no status of its own; a VERSION of its API has a state. This
 * card shows the one state that can be read today: the version running in production.
 *
 * The tile reads the model's own discovery (GET /owl/): each served class reports the fingerprint
 * of the shape its API was generated from. The truth each test holds the page to is asked of the
 * door directly, never through the page's lookup:
 *   - one served class: the tile shows that class's version, "in production";
 *   - several: it says how many classes report a version; each class's own is in the Vocabulary fold;
 *   - a served class that reports no version: "version not reported", never "in production";
 *   - no served class: "no API in production", never a version;
 *   - open cards on the domain never change it, and a typed status on the row is never shown.
 *
 * Page code this suite opens and reads: platform/api/public/athena/domain.html.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-true-5034.spec.cjs -g "#5045"
 */
const { test, expect } = require('@playwright/test');
const { BASE, PHONE, openDomainPage, serveLintOnce, serveOwnRoutes, serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');

const BEFORE_5045 = '053b45343dd35620691020bca43d17629c46aa54';

/** What the doors hold for a domain: the classes it declares that are served, each with the version it reports (or null). */
async function apiTruth(page, domain) {
  const get = async (p) => { const r = await page.request.get(BASE + p, { timeout: 20_000 }); expect(r.ok(), `GET ${p}: HTTP ${r.status()}`).toBe(true); return r.json(); };
  const row = ((await get('/owl/domains/domains?limit=200')).data || []).find((r) => r.name === domain);
  expect(row, `the model holds no domain named ${domain}`).toBeTruthy();
  const listed = (await get('/owl/')).primitives || [];
  return [].concat(row.definesVocabulary || []).filter(Boolean)
    .map((kind) => listed.find((p) => p.kind === kind)).filter(Boolean)
    .map((p) => ({ kind: p.kind, version: p.shapeVersion && p.shapeVersion !== 'unversioned' ? String(p.shapeVersion) : null }));
}

/** The tile as drawn: { state, value, label }, or null; and each Vocabulary row's text by class. */
const drawn = (page) => page.evaluate(() => {
  const clean = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const t = document.querySelector('#main .stats .stat[data-api]');
  const vocab = Object.fromEntries([...document.querySelectorAll('#main details.fold')].filter((f) => /^\s*Vocabulary/.test(f.querySelector('summary')?.textContent || ''))
    .flatMap((f) => [...f.querySelectorAll('tr')].filter((tr) => tr.querySelector('td')).map((tr) => [clean(tr.cells[0]), clean(tr.cells[1])])));
  const labels = [...document.querySelectorAll('#main .stats .stat .l')].map(clean);
  return { tile: t ? { state: t.getAttribute('data-api'), value: clean(t.querySelector('.v')), label: clean(t.querySelector('.l')) } : null, vocab, labels };
});

/** Draw the page again and wait for it. A route added after the page was opened is the one that answers. */
async function redraw(page) {
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#main .stats .stat[data-api]') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
}

/**
 * Answer the page's reads of discovery with the live answer, changed for one class by `change(entry)`.
 * Added AFTER the page is opened (the lib registers its own discovery route on open, and the later
 * route wins), then the page is drawn again.
 */
async function withDiscovery(page, change) {
  await page.route((url) => url.origin === new URL(BASE).origin && url.pathname === '/owl/', async (route) => {
    const live = await route.fetch();
    const body = await live.json();
    body.primitives = (body.primitives || []).map((p) => change({ ...p })).filter(Boolean);
    return route.fulfill({ response: live, json: body });
  });
}

test.describe('#5045 AC1 the tile shows the API version in production, never a typed status', () => {
  test.setTimeout(240_000);

  test('a domain serving one class shows that class\'s version, read live, "in production"', async ({ page }) => {
    const truth = await apiTruth(page, 'logs');
    expect(truth.length, 'logs no longer serves exactly one class: pick another live case').toBe(1);
    expect(truth[0].version, 'the logs API reports no version: pick another live case').toBeTruthy();
    await openDomainPage(page, { domain: 'logs' });
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'live', value: truth[0].version.slice(0, 8), label: 'API version · in production' });
    expect(d.vocab[truth[0].kind], 'the class\'s own row names the same version').toContain(`version ${truth[0].version.slice(0, 8)}`);
    expect(d.labels, 'no tile is labelled Status any more').not.toContain('Status');
  });

  test('a domain serving several classes says how many report a version; each is named in Vocabulary', async ({ page }) => {
    const truth = await apiTruth(page, 'alerts');
    expect(truth.length, 'alerts no longer serves several classes: pick another live case').toBeGreaterThan(1);
    const versioned = truth.filter((t) => t.version);
    await openDomainPage(page, { domain: 'alerts' });
    const d = await drawn(page);
    expect(d.tile.state).toBe('live');
    expect(d.tile.value).toBe(versioned.length === truth.length ? `${truth.length} classes` : `${versioned.length} of ${truth.length} classes`);
    expect(d.tile.label).toBe('API versions · in production');
    for (const t of truth) expect(d.vocab[t.kind], `${t.kind}'s row`).toContain(t.version ? `version ${t.version.slice(0, 8)}` : 'version not reported');
  });

  test('NEGATIVE: a served class that reports no version reads "version not reported", never "in production"', async ({ page }) => {
    // logs' one class, with the version taken out of the door's own answer (principles is in this state for real today)
    const kind = (await apiTruth(page, 'logs'))[0].kind;
    await openDomainPage(page, { domain: 'logs' });
    await withDiscovery(page, (p) => { if (p.kind === kind) delete p.shapeVersion; return p; });
    await redraw(page);
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'unreported', value: 'version not reported', label: 'API version' });
    expect(d.vocab[kind]).toContain('version not reported');
  });

  test('NEGATIVE: the API\'s own word "unversioned" is not shown as a version', async ({ page }) => {
    const kind = (await apiTruth(page, 'logs'))[0].kind;
    await openDomainPage(page, { domain: 'logs' });
    await withDiscovery(page, (p) => { if (p.kind === kind) p.shapeVersion = 'unversioned'; return p; });
    await redraw(page);
    expect((await drawn(page)).tile).toEqual({ state: 'unreported', value: 'version not reported', label: 'API version' });
  });

  test('NEGATIVE: a domain that serves no class reads "no API in production" and shows no version', async ({ page }) => {
    const truth = await apiTruth(page, 'wine-diary-wines');
    expect(truth, 'wine-diary-wines now serves a class: pick another live case').toEqual([]);
    await openDomainPage(page, { domain: 'wine-diary-wines' });
    expect((await drawn(page)).tile).toEqual({ state: 'none', value: 'no API in production', label: 'API version' });
  });

  test('NEGATIVE: a status typed on the row is never shown, and the open cards do not change the tile', async ({ page }) => {
    const truth = await apiTruth(page, 'logs');
    // the Domain row as served, with a typed status put back on it
    await page.route((url) => url.origin === new URL(BASE).origin && /\/owl\/domains(\/logs)?$/.test(url.pathname), async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      for (const row of [].concat(body.data || [])) if (row && row.name === 'logs') row.status = 'retiring-typed-5045';
      return route.fulfill({ response: live, json: body });
    });
    // and a board with an open card on the domain
    await page.route((url) => /\/api\/chorus\/domain\/logs\/cards$/.test(url.pathname), (route) => route.fulfill({ status: 200, contentType: 'application/json',
      body: JSON.stringify({ domain: 'logs', cards: [{ id: '1', title: 'an open card planted by the test', owner: 'wren', status: 'WIP', priority: 'P1' }] }) }));
    await openDomainPage(page, { domain: 'logs' });
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'live', value: truth[0].version.slice(0, 8), label: 'API version · in production' });
    expect(await page.evaluate(() => document.querySelector('#main').innerText), 'the typed status reached the page').not.toContain('retiring-typed-5045');
    expect(await page.evaluate(() => document.querySelector('#main').innerText), 'the planted card was not drawn: this proof needs it on the page').toContain('an open card planted by the test');
  });

  test('NEGATIVE: the page as it was before this card shows the typed word, and this check names it', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await serveLintOnce(page);
    await serveOwnRoutes(page);
    await serveAthenaFilesAsOf(page, BASE, BEFORE_5045);
    await page.goto(`${BASE}/athena/domain.html?d=principles`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#main .stats') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
    const d = await drawn(page);
    expect(d.tile, 'the old page has no version tile').toBeNull();
    expect(d.labels, 'the old page did not label a tile Status: this proof no longer shows the fault').toContain('Status');
  });
});
}

// =============================================================================================
// #5042 AC3 — Jeff on the principles page, 2026-11-25 10:31: "api contract must be link to
// generated open api doc". The API Contract fold links to the OpenAPI document each of the
// domain's collections serves, and lists the routes that document describes. It listed the code
// crawl's endpoints: chorus-api's old Loom routes (/api/loom/principles, /loom, /loom/:role).
// Read from the drawn page through the sign-in door; the link is followed through the door too.
// Before the land the live door serves no document: lib/landed-openapi-5042.json stands in the
// landed answers (the merge's entry and the generator's own file), off in the nightly.
// NEGATIVE: the page as it was before this card is red, named, on the same reads.
{
  const { startDoor, signedInPage, LANDED_OPENAPI: LANDED } = require('./lib/athena-page-5034.cjs');
  const fsx = require('fs');
  const pathx = require('path');
  const BEFORE_5042 = 'bb3e8d009390bc6ab5ed1515b8a09c659f6705a5';

  /** The API Contract fold as drawn: its document links and its route rows. */
  const drawnContract = () => {
    const fold = [...document.querySelectorAll('#main details.fold')].find((f) => /^\s*API Contract/.test(f.querySelector('summary')?.textContent || ''));
    if (!fold) return null;
    const clean = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
    return {
      links: [...fold.querySelectorAll('a[data-openapi]')].map((a) => a.href),
      rows: [...fold.querySelectorAll('tr')].filter((tr) => tr.querySelector('td')).map((tr) => `${clean(tr.cells[0])} ${clean(tr.cells[1])}`),
    };
  };

  /** What is wrong with the fold, given each linked document as the door answered it. */
  function contractFaults(drawn, docs) {
    if (!drawn) return ['the page draws no API Contract fold'];
    const faults = [];
    if (!drawn.links.length) faults.push('the fold links to no OpenAPI document');
    const want = [];
    for (const [href, d] of Object.entries(docs)) {
      if (d.status !== 200 || d.body?.openapi !== '3.1.0') { faults.push(`${href} answered ${d.status}, not an OpenAPI 3.1 document`); continue; }
      for (const [route, ops] of Object.entries(d.body.paths || {})) for (const m of Object.keys(ops)) want.push(`${m.toUpperCase()} ${route}`);
    }
    const extra = drawn.rows.filter((r) => !want.includes(r));
    const missing = want.filter((r) => !drawn.rows.includes(r));
    if (extra.length) faults.push(`the fold lists routes no linked document describes: ${extra.join(', ')}`);
    if (missing.length) faults.push(`the fold leaves out routes its document describes: ${missing.join(', ')}`);
    return faults;
  }

  /** Stand in the landed discovery entry and document under the door (off in the nightly: LANDED is empty). */
  async function landedThroughDoor(page) {
    for (const l of LANDED) {
      await page.route((url) => url.pathname.endsWith('/owl' + l.openapi), (route) =>
        route.fulfill({ status: 200, contentType: 'application/json', body: fsx.readFileSync(pathx.resolve(__dirname, '..', '..', l.file), 'utf8') }));
    }
    if (!LANDED.length) return;
    await page.route((url) => url.pathname.endsWith('/owl/'), async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      for (const p of body.primitives || []) { const l = LANDED.find((x) => x.kind === p.kind); if (l && p.openapi === undefined) p.openapi = l.openapi; }
      return route.fulfill({ response: live, json: body });
    });
  }

  async function readContract(page, door) {
    await page.waitForFunction(() => [...document.querySelectorAll('#main details.fold summary')].some((s) => /^\s*API Contract \(\d+\)/.test(s.textContent)), null, { timeout: 180_000 });
    const drawn = await page.evaluate(drawnContract);
    const docs = {};
    for (const href of drawn?.links || []) {
      // followed as the reader's click would be: through the door, signed in (the stand-in answers it before the land)
      const body = await page.evaluate(async (u) => { const r = await fetch(u); return { status: r.status, body: await r.json().catch(() => null) }; }, href);
      docs[href] = body;
    }
    return { drawn, docs };
  }

  test.describe('#5042 AC3 the API Contract fold links to the generated OpenAPI document', () => {
    test.setTimeout(420_000);
    let door;
    test.beforeAll(async () => { door = await startDoor({ mount: '/chorus' }); });
    test.afterAll(async () => { if (door) door.stop(); });

    test('through the sign-in door: the fold links each document, the link answers it, and the rows are its routes', async ({ browser }) => {
      const { page, context } = await signedInPage(browser, door);
      try {
        await landedThroughDoor(page);
        await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=principles`, { waitUntil: 'domcontentloaded' });
        const { drawn, docs } = await readContract(page, door);
        const faults = contractFaults(drawn, docs);
        expect(faults, faults.join('\n')).toEqual([]);
        expect(drawn.links.every((h) => h.startsWith(door.origin + door.mount + '/owl/')), `a document link leaves the door: ${drawn.links}`).toBe(true);
        expect(drawn.rows.some((r) => /\/api\/loom/.test(r)), `the old Loom routes are still listed: ${drawn.rows}`).toBe(false);
      } finally { await context.close(); }
    });

    // the dead-link check reads the stand-in for a stood-in route only (lib deadLinkReason): it replaces
    // the live GET of /owl/v1/principles/principles/openapi.json, which answers 404 until the land
    test('NEGATIVE: the dead-link check still names a route that opens nothing, and a stood-in route whose document is gone', async ({ page }) => {
      const { deadLinks } = require('./lib/athena-page-5034.cjs');
      const dead = await deadLinks(page, [`${BASE}/owl/v1/principles/nosuch/openapi.json`]);
      expect(dead.join('\n'), 'a route no door answers was not named dead').toMatch(/nosuch\/openapi\.json — HTTP 404$/m);
      if (!LANDED.length) return;   // the nightly: nothing is stood in, the live door answers
      const file = LANDED[0].file;
      LANDED[0].file = 'shaping/no-such-5042.openapi.json';
      try {
        const gone = await deadLinks(page, [`${BASE}/owl${LANDED[0].openapi}`]);
        expect(gone.join('\n'), 'a stood-in route whose document is gone was not named dead').toMatch(/the stood-in document does not read/);
      } finally { LANDED[0].file = file; }
      expect(await deadLinks(page, [`${BASE}/owl${LANDED[0].openapi}`]), 'the stood-in route with its document is dead').toEqual([]);
    });

    test('NEGATIVE: the fold as it was before this card is red, named', async ({ browser }) => {
      const { page, context } = await signedInPage(browser, door);
      try {
        await landedThroughDoor(page);
        const { serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');
        await serveAthenaFilesAsOf(page, door.origin + door.mount, BEFORE_5042);
        await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=principles`, { waitUntil: 'domcontentloaded' });
        const { drawn, docs } = await readContract(page, door);
        const faults = contractFaults(drawn, docs);
        expect(faults, 'the old fold passed: this check cannot tell it from the fix').toContain('the fold links to no OpenAPI document');
      } finally { await context.close(); }
    });
  });
}
