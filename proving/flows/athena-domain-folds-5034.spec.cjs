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
    await page.waitForFunction(() => !/running the domain lint/.test(document.getElementById('conf-slot')?.textContent || ''), null, { timeout: 180_000 });   // 90 s was not enough inside the pipeline's test step (run 2, 11-24 17:21)
    // (#5041: the ops view no longer embeds the Model fold, so there is no model slot to wait for)
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

// =============================================================================================
// #5040 — the same question, asked where Jeff stands: through the sign-in door.
//
// Jeff, 2026-11-25 08:06, five phone screenshots from the car: "the page is way too broken to do
// an interview". At that minute all 13 browser tests of #5034 were green. They open the pages at
// localhost:3340, at the root, with no door. He opens them at the public address:
//   - lightlifeurbangardens.com/chorus/…   the door mounted under /chorus
//   - chorus.lightlifeurbangardens.com/…   the door at the root of its own host
// Both are behind chorus-share-guard: an allow list, a session, GET and HEAD only.
//
// lib startDoor runs the real door on a scratch port for each mount, signs a test person in with
// the door's own code, and the three principles pages are opened at phone width. At BOTH mounts:
//   (a) every fold is filled or shows its true reason; nothing the page asks for leaves the door
//       or is refused by it; the model view draws; the rows page lists every row the model holds;
//   (b) every link drawn stays inside the door and leads somewhere;
//   (c) the Services and Code folds read through the door with no POST, and show what the model
//       holds. The door stays GET/HEAD only. (#5041: the files are listed once, in the Code fold;
//       the separate Files fold this check first read is gone.)
// NEGATIVE PROOFS: the pages as they were before #5040 (cut from git) are red under /chorus,
// named fold by fold; data asked outside the mount does not answer.
//
// Page code these tests open and read, named so a change to any of it runs this file:
//   platform/api/public/athena/domain.html, platform/api/public/athena/domains-view.html,
//   platform/api/public/athena/facets.js, platform/api/public/athena/model-fold.js,
//   platform/api/public/athena/rows.js, platform/api/public/athena/athena-flow.js.
// (They live in this file, beside the fold checks they extend: the layout rule counts files
// under proving/flows, and a third lib-and-spec pair would have gone over its limit.)
//
// Named limits: the door's upstream is the live chorus-api, so an endpoint change is not tested
// here before it lands; this is a desktop browser engine on the Library machine, not a phone on
// a cell connection, and the read budget below is the only stand-in for one.
// =============================================================================================
const { execFileSync } = require('child_process');
const { startDoor, signedInPage, BASE, ROOT } = require('./lib/athena-page-5034.cjs');

const MOUNTS = [['under /chorus (the public apex)', '/chorus'], ['at the root (the door\'s own host)', '']];
const DOOR_NO_ANSWER = /could not read|cannot reach|cannot measure|cannot read|fetch failure|no answer|load failed|HTTP \d{3}|loading the materialized view|loading live from the model|running the domain lint/i;
// Jeff's phone gave up on the 14 to 19 s whole-table read (his Code fold, 08:16). 10 s is well
// under that and well over what a healthy read takes here (under 1 s), so a busy machine does
// not turn the suite red. The policy lint runs a real measurement and is left out of the budget.
const READ_BUDGET_MS = 10_000;
// the commit before #5040 touched the pages: what Jeff's phone was showing at 08:02
const BEFORE_5040 = '112388152d0cfe0bd1d4d227c41c051170ce85a9';

/** Open a page through the door and wait until nothing on it is still loading. */
async function openThroughDoor(page, door, pathAndQuery) {
  await page.goto(door.origin + door.mount + pathAndQuery, { waitUntil: 'domcontentloaded' });
  // innerText, never textContent: the page's own inline scripts contain these very phrases
  await page.waitForFunction(() => !/loading the materialized view|loading live from the model|Loading the model|running the domain lint/.test(document.body.innerText)
    // the shared model fold says when it has drawn (model-fold.js sets data-mfold-ready); until then its slot is empty
    && [...document.querySelectorAll('#mfold-slot')].every((s) => s.dataset.mfoldReady === '1')
    && ![...document.querySelectorAll('[data-mf]')].some((n) => n.textContent.trim() === '…'), null, { timeout: 180_000 });
  await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
}

/** Run in the page: every fold whose drawn text is a failure or a placeholder, as "name: what it says". */
function brokenFolds(pattern) {
  const re = new RegExp(pattern, 'i');
  const out = [];
  for (const f of document.querySelectorAll('details.fold, details.mfwrap')) {
    const name = (f.querySelector('summary')?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 40);
    const said = (f.textContent || '').replace(/\s+/g, ' ').match(re);
    if (said) out.push(`${name}: ${said[0]}`);
  }
  const note = document.querySelector('#app > .note, #main .err, .mferr');
  if (note) out.push(`page: ${note.textContent.trim().replace(/\s+/g, ' ').slice(0, 90)}`);
  return out;
}

/** What the store itself holds, asked directly on localhost: the truth the drawn page is held to. */
async function storeRows(request, query) {
  const r = await request.post(`${BASE}/sparql-read`, { timeout: 30_000, headers: { Accept: 'application/sparql-results+json' }, data: { query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> ${query}` } });
  if (!r.ok()) throw new Error(`the store did not answer: HTTP ${r.status()}`);
  return (await r.json()).results.bindings;
}
const principleCount = async (request) => Number((await storeRows(request, 'SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a chorus:Principle } }'))[0].n.value);

for (const [where, mount] of MOUNTS) {
  test.describe(`#5040 the ${DOMAIN} pages through the sign-in door, ${where}`, () => {
    test.setTimeout(300_000);
    let door;
    test.beforeAll(async () => { door = await startDoor({ mount }); });
    test.afterAll(() => door && door.stop());

    test('the ops view: every fold is filled or says a true reason, and nothing leaves or is refused by the door', async ({ browser }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      const slow = [];
      page.on('requestfinished', (r) => { const t = r.timing(); if (t.responseEnd > READ_BUDGET_MS) slow.push(`${Math.round(t.responseEnd / 100) / 10} s ${r.url().replace(door.origin, '')}`); });
      await openThroughDoor(page, door, `/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`);
      const broken = await page.evaluate(brokenFolds, DOOR_NO_ANSWER.source);
      expect.soft(broken, `folds that failed through the door:\n${broken.join('\n')}`).toEqual([]);
      expect.soft([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      const tooSlow = slow.filter((s) => !/\/api\/chorus\/policies\/counts/.test(s));
      expect.soft(tooSlow, `reads over ${READ_BUDGET_MS / 1000} s, which a phone connection gives up on:\n${tooSlow.join('\n')}`).toEqual([]);
      const folds = await page.evaluate(() => document.querySelectorAll('details.fold').length);
      expect(folds, 'the page drew its folds').toBeGreaterThan(12);
      await context.close();
    });

    // #5041 — this test replaces the hand-run script platform/tests/test-model-fold-drift.cjs, retired
    // on this card. That script compared the model fold on two surfaces (#3757); the ops view no
    // longer embeds it, so one surface is left. What the script still held of the model view (reached
    // by its own address, at least one class drawn) is held here, through the door, at both mounts.
    test('the model view draws the schema and a card for every row, through the door', async ({ browser, request }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      await openThroughDoor(page, door, `/domains/${encodeURIComponent(DOMAIN)}`);
      expect.soft(new URL(page.url()).pathname, 'the model view is reached by its own address, with no redirect').toBe(`${door.mount}/domains/${encodeURIComponent(DOMAIN)}`);
      expect.soft(await page.locator('[data-mf="schema"] h3').count(), 'the schema draws at least one class').toBeGreaterThan(0);
      const broken = await page.evaluate(brokenFolds, DOOR_NO_ANSWER.source);
      expect.soft(broken, `the model view failed through the door:\n${broken.join('\n')}`).toEqual([]);
      expect.soft([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      const held = await principleCount(request);
      expect(await page.locator('[data-mf="instances"] a.mfcard').count(), `the model holds ${held} Principle rows`).toBe(held);
      await context.close();
    });

    test('the rows page lists every row the model holds, through the door', async ({ browser, request }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      await openThroughDoor(page, door, '/athena/rows.html?kind=Principle');
      const held = await principleCount(request);
      expect.soft(await page.locator('#main table.rows tbody tr').count(), `the model holds ${held} rows`).toBe(held);
      expect([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      await context.close();
    });

    test('every link on the ops view and the model view stays inside the door and leads somewhere', async ({ browser }) => {
      const { page, context } = await signedInPage(browser, door);
      const dead = [];
      for (const p of [`/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, `/domains/${encodeURIComponent(DOMAIN)}`]) {
        await openThroughDoor(page, door, p);
        const links = await page.evaluate(() => [...new Set([...document.querySelectorAll('a[href]')].map((a) => a.href))].filter((h) => /^https?:/.test(h)));
        expect(links.length, `${p} drew no links`).toBeGreaterThan(5);
        for (const h of links) {
          if (!h.startsWith(door.origin + '/')) continue; // another site: not this door's to answer
          if (!h.startsWith(door.origin + door.mount + '/') && h !== door.origin + door.mount) { dead.push(`${p}: ${h.replace(door.origin, '')} — leaves the door`); continue; }
          const r = await context.request.get(h, { timeout: 30_000, maxRedirects: 0 }).catch((e) => ({ status: () => 0, _e: String(e.message).split('\n')[0] }));
          if (r.status() !== 200) dead.push(`${p}: ${h.replace(door.origin, '')} — ${r.status() || r._e}`);
        }
      }
      expect(dead, `links that leave the door or lead nowhere:\n${dead.join('\n')}`).toEqual([]);
      await context.close();
    });

    // #5041 — was "the Services and Files folds…". Jeff's ruling of 2026-11-25 ("files seems same as
    // code fold?"): the files are listed once, in the Code fold (#code-slot). The assertion is the
    // same one, read from the fold that now holds the rows: exactly the files the store holds.
    test('the Services and Code folds read through the door with no POST, and show what the store holds', async ({ browser, request }) => {
      const { page, context } = await signedInPage(browser, door);
      const posts = [];
      page.on('request', (r) => { if (r.method() !== 'GET' && r.method() !== 'HEAD') posts.push(`${r.method()} ${r.url().replace(door.origin, '')}`); });
      await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
      await page.waitForFunction(() => document.querySelector('#svc-slot details') && document.querySelector('#code-slot details'), null, { timeout: 120_000 });
      const drawn = await page.evaluate(() => ({
        services: document.querySelector('#svc-slot').innerText.replace(/\s+/g, ' ').trim(),
        serviceNames: [...document.querySelectorAll('#svc-slot h3 a')].map((a) => a.textContent.trim()).sort(),
        files: [...document.querySelectorAll('#code-slot td')].map((t) => t.textContent.trim()).sort(),
        filesSummary: document.querySelector('#code-slot summary').textContent.replace(/\s+/g, ' ').trim() }));
      await context.close();
      expect.soft(posts, `the page must only read (GET) through the door:\n${posts.join('\n')}`).toEqual([]);
      // the truth is the store's own answer, asked directly on localhost: the two queries the folds used to POST
      const hosted = (await storeRows(request, `SELECT DISTINCT ?svc WHERE { GRAPH ?g { <https://jeffbridwell.com/chorus#${DOMAIN}> chorus:hosts ?svc } }`))
        .map((b) => String(b.svc.value).split('#').pop().replace(/^service-/, '')).sort();
      expect.soft(drawn.services, 'the Services fold').not.toMatch(DOOR_NO_ANSWER);
      expect.soft(drawn.serviceNames, `the Services fold names exactly the services the store says this domain hosts (it says: ${drawn.services.slice(0, 120)})`).toEqual(hosted);
      const held = (await storeRows(request, `SELECT DISTINCT ?path WHERE { GRAPH ?g { ?f a chorus:CodeFile ; chorus:hasDomain <https://jeffbridwell.com/chorus#${DOMAIN}> ; chorus:filePath ?path } }`))
        .map((b) => String(b.path.value)).sort();
      expect(held.length, 'the store holds files for this domain').toBeGreaterThan(0);
      expect.soft(drawn.filesSummary, 'the Code fold').not.toMatch(/\(\?\)/);
      expect(drawn.files, 'the Code fold through the door lists exactly the files the store holds').toEqual(held);
    });
  });
}

test.describe('#5040 NEGATIVE: the door checks are red on the pages that broke on Jeff\'s phone', () => {
  test.setTimeout(300_000);
  test('the ops view as it was before #5040, under /chorus, is named fold by fold', async ({ browser }) => {
    const before = (f) => execFileSync('git', ['-C', ROOT, 'show', `${BEFORE_5040}:platform/api/public/athena/${f}`], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    const door = await startDoor({ mount: '/chorus' });
    try {
      const { page, context, refused } = await signedInPage(browser, door);
      await page.route((url) => url.origin === door.origin && /^\/chorus\/athena\/(domain\.html|facets\.js|model-fold\.js)$/.test(url.pathname), (route) => {
        const f = new URL(route.request().url()).pathname.split('/').pop();
        return route.fulfill({ status: 200, contentType: f.endsWith('.html') ? 'text/html; charset=utf-8' : 'text/javascript; charset=utf-8', body: before(f) });
      });
      await page.goto(`${door.origin}/chorus/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
      // wait until the folds have drawn their answer, failure or not (the page opens on a placeholder)
      await page.waitForFunction(() => document.querySelector('#facets details.fold') && document.querySelector('#svc-slot details') && document.querySelector('#files-slot details')
        && !/loading the materialized view|loading live from the model|running the domain lint/.test(document.body.innerText), null, { timeout: 120_000 });
      await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
      const broken = await page.evaluate(brokenFolds, DOOR_NO_ANSWER.source);
      for (const fold of ['Conformance', 'Services', 'Files', 'Cards', 'Code']) expect(broken.join('\n'), `the old page must be caught on its ${fold} fold`).toMatch(new RegExp(`^${fold}`, 'm'));
      expect(refused.join('\n'), 'the old page asks outside the door').toMatch(/LEFT THE DOOR GET \/api\/chorus\/domain\//);
      expect(refused.join('\n'), 'the old page POSTs, which the door refuses').toMatch(/405 POST \/chorus\/sparql-read/);
      await context.close();
    } finally { door.stop(); }
  });

  test('data asked outside the mount does not answer, and the same link inside it does', async ({ browser }) => {
    const door = await startDoor({ mount: '/chorus' });
    try {
      const { context } = await signedInPage(browser, door);
      const inside = await context.request.get(`${door.origin}/chorus/athena/domains.html`, { maxRedirects: 0 });
      const outside = await context.request.get(`${door.origin}/owl/v1/principles/principles`, { maxRedirects: 0 });
      expect(inside.status(), 'a link inside the door answers').toBe(200);
      expect(outside.status(), 'the same data asked outside the mount does not answer: that is the vocabulary link of 08:02').not.toBe(200);
      await context.close();
    } finally { door.stop(); }
  });
});

// =============================================================================================
// #5041 AC4 — the folds come in the order a reader needs, and the long healthy ones start closed.
//
// Borg Queen's UX review of Jeff's walk (2026-11-25 10:44), taken by Wren: what the domain is and
// exposes (Vocabulary, API Contract, Services), then whether it is healthy (Conformance, Gaps,
// Dependencies, Design doc, Decisions), then the work and the code (UI Pages, Alerts, Logs,
// Cards, Pipeline, Releases, Infrastructure, Tests, Code). Conformance starts closed when no
// policy is red and open when one is; Code, the longest fold, starts closed.
//
// Read from the page as it STARTS: nothing is opened by the test before the folds are read.
// NEGATIVE: a page with a red policy shows Conformance open, and the same page with the fold
// shut is red; a green page with the fold forced open is red; the page Jeff walked is red on
// its order.
//
// Page code read as drawn: platform/api/public/athena/domain.html (the slots, the Conformance
// and Code folds), platform/api/public/athena/facets.js (the order of the shared folds).
// =============================================================================================
const { serveOwnAthenaFiles, openOldDomainPage, PHONE } = require('./lib/athena-page-5034.cjs');

const FOLD_ORDER = ['Vocabulary', 'API Contract', 'Services', 'Conformance', 'Gaps', 'Dependencies', 'Design doc', 'Decisions',
  'UI Pages', 'Alerts', 'Logs', 'Cards', 'Pipeline', 'Releases', 'Infrastructure', 'Tests', 'Code'];
// a fold the page draws only when there is something to put in it: a domain with no design document has none
const MAY_BE_ABSENT = ['Design doc'];

/** Run in the page: every fold top to bottom, as it stands: its name, whether it is open, the red policies in it. */
function foldsAsDrawn() {
  return [...document.querySelectorAll('details.fold, details.mfwrap')].map((f) => ({
    name: (f.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim().replace(/\s*\(.*$/, ''),
    open: f.open,
    red: f.querySelectorAll('tr[data-verdict]:not([data-verdict="pass"])').length,
    policies: f.querySelectorAll('tr[data-verdict]').length,
    unread: /\(\?\)/.test(f.querySelector(':scope > summary')?.textContent || ''),
  }));
}

/** Every way the folds are out of the reader's order, or start open or closed against the rule. */
function orderFaults(folds) {
  const out = [];
  // "Services and running units" is the Services fold: a drawn name is the listed name, or starts with it
  const known = (n) => FOLD_ORDER.find((e) => n === e || n.startsWith(e + ' '));
  const drawn = folds.map((f, i) => ({ ...f, as: known(f.name), i }));
  for (const f of drawn) {
    if (f.name === 'Description') { if (f.i !== 0) out.push(`Description is fold ${f.i + 1}: when drawn it sits first, under the purpose`); continue; }
    if (!f.as) out.push(`a fold the order does not name: "${f.name}"`);
  }
  const seq = drawn.filter((f) => f.as).map((f) => f.as);
  for (const e of FOLD_ORDER) if (!seq.includes(e) && !MAY_BE_ABSENT.includes(e)) out.push(`the ${e} fold is not drawn`);
  for (const e of new Set(seq)) if (seq.filter((s) => s === e).length > 1) out.push(`the ${e} fold is drawn ${seq.filter((s) => s === e).length} times`);
  const want = FOLD_ORDER.filter((e) => seq.includes(e));
  if (JSON.stringify([...new Set(seq)]) !== JSON.stringify(want)) out.push(`order: drawn ${seq.join(' > ')}; it must be ${want.join(' > ')}`);
  const conf = drawn.find((f) => f.as === 'Conformance');
  if (conf && conf.policies && conf.open !== (conf.red > 0)) out.push(`Conformance starts ${conf.open ? 'open' : 'closed'} with ${conf.red} red of ${conf.policies}: it is open when a policy is red, closed when none is`);
  const code = drawn.find((f) => f.as === 'Code');
  if (code && code.open && !code.unread) out.push('Code starts open: the longest fold on the page starts closed');
  return out;
}

/** Open the ops view at phone width and wait until every fold has drawn. Nothing is opened. */
async function openAsItStarts(page) {
  await page.setViewportSize(PHONE);
  await serveOwnAthenaFiles(page);
  await page.goto(`${BASE}/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#code-slot details') && document.querySelector('#svc-slot details') && document.querySelector('#conf-slot details')
    && document.querySelector('#facets details.fold') && !/running the domain lint/.test(document.getElementById('conf-slot').textContent), null, { timeout: 180_000 });
}

/** The policies endpoint's answer, written by the test: the named policies with the verdicts given. */
const policiesAnswer = (verdicts) => (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ measuredAt: '2026-11-25T16:00:00.000Z',
  policies: verdicts.map((verdict, i) => ({ name: `fixture-policy-${i + 1}`, label: 'planted by the test', verdict, count: verdict === 'pass' ? 0 : 3, baseline: 0, scope: 'global', violations: verdict === 'pass' ? [] : ['one thing wrong'] })) }) });
const isPoliciesRead = (url) => url.pathname === '/api/chorus/policies/counts';

test.describe(`#5041 the folds of the ${DOMAIN} ops view come in the reader's order and start as they should`, () => {
  test.setTimeout(300_000);

  test('the folds are drawn in the reader\'s order; Conformance is closed unless a policy is red; Code starts closed', async ({ page }) => {
    await openAsItStarts(page);
    const folds = await page.evaluate(foldsAsDrawn);
    expect(folds.length, 'the page drew too few folds to be the domain page').toBeGreaterThan(12);
    const faults = orderFaults(folds);
    expect(faults, `the folds are out of order or start wrong:\n${faults.join('\n')}`).toEqual([]);
    expect(folds.find((f) => f.name === 'Code').open, 'Code starts closed').toBe(false);
  });

  test('with no red policy Conformance starts closed; with one it starts open. NEGATIVE: either turned round is red', async ({ page }) => {
    await page.route(isPoliciesRead, policiesAnswer(['pass', 'pass', 'pass']));
    await openAsItStarts(page);
    let folds = await page.evaluate(foldsAsDrawn);
    expect(folds.find((f) => f.name === 'Conformance'), 'three green policies: closed').toMatchObject({ open: false, red: 0, policies: 3 });
    expect(orderFaults(folds)).toEqual([]);
    // NEGATIVE: the green fold forced open is named
    await page.evaluate(() => { document.querySelector('#conf-slot details').open = true; });
    expect(orderFaults(await page.evaluate(foldsAsDrawn))).toEqual(['Conformance starts open with 0 red of 3: it is open when a policy is red, closed when none is']);

    await page.unroute(isPoliciesRead);
    await page.route(isPoliciesRead, policiesAnswer(['pass', 'fail', 'pass']));
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#conf-slot tr[data-verdict]') && document.querySelector('#code-slot details') && document.querySelector('#facets details.fold'), null, { timeout: 120_000 });
    folds = await page.evaluate(foldsAsDrawn);
    expect(folds.find((f) => f.name === 'Conformance'), 'one red policy: open').toMatchObject({ open: true, red: 1, policies: 3 });
    expect(orderFaults(folds)).toEqual([]);
    // NEGATIVE: a red policy behind a closed fold is named
    await page.evaluate(() => { document.querySelector('#conf-slot details').open = false; });
    expect(orderFaults(await page.evaluate(foldsAsDrawn))).toEqual(['Conformance starts closed with 1 red of 3: it is open when a policy is red, closed when none is']);
    // NEGATIVE: and Code left open is named
    await page.evaluate(() => { document.querySelector('#conf-slot details').open = true; document.querySelector('#code-slot details').open = true; });
    expect(orderFaults(await page.evaluate(foldsAsDrawn))).toEqual(['Code starts open: the longest fold on the page starts closed']);
  });

  test('NEGATIVE: the page Jeff walked is red on its order, and a fold moved or missing is named', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await openOldDomainPage(page, BASE);
    const faults = orderFaults(await page.evaluate(foldsAsDrawn)).join('\n');
    expect(faults, 'the old order').toMatch(/^order: drawn Vocabulary > Design doc > Gaps > Conformance > Services > Cards > /m);
    expect(faults, 'the second list of files').toMatch(/^a fold the order does not name: "Files"$/m);
    expect(faults, 'the embedded Model fold').toMatch(/^a fold the order does not name: "Model/m);
    expect(faults, 'the old Code fold started open').toMatch(/^Code starts open/m);
    // and by hand: two folds swapped, and one taken out
    const right = FOLD_ORDER.map((name) => ({ name, open: false, red: 0, policies: 0, unread: false }));
    expect(orderFaults(right)).toEqual([]);
    const swapped = [...right]; [swapped[4], swapped[5]] = [swapped[5], swapped[4]];
    expect(orderFaults(swapped).join('\n')).toMatch(/^order: drawn .*Dependencies > Gaps/);
    expect(orderFaults(right.filter((f) => f.name !== 'Pipeline'))).toEqual(['the Pipeline fold is not drawn']);
    expect(orderFaults(right.filter((f) => f.name !== 'Design doc')), 'a domain with no design document draws no fold for it').toEqual([]);
  });
});
