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
//   (c) the Services and Files folds read through the door with no POST, and show what the model
//       holds. The door stays GET/HEAD only.
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

    test('the model view draws the schema and a card for every row, through the door', async ({ browser, request }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      await openThroughDoor(page, door, `/domains/${encodeURIComponent(DOMAIN)}`);
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

    test('the Services and Files folds read through the door with no POST, and show what the store holds', async ({ browser, request }) => {
      const { page, context } = await signedInPage(browser, door);
      const posts = [];
      page.on('request', (r) => { if (r.method() !== 'GET' && r.method() !== 'HEAD') posts.push(`${r.method()} ${r.url().replace(door.origin, '')}`); });
      await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
      await page.waitForFunction(() => document.querySelector('#svc-slot details') && document.querySelector('#files-slot details'), null, { timeout: 120_000 });
      const drawn = await page.evaluate(() => ({
        services: document.querySelector('#svc-slot').innerText.replace(/\s+/g, ' ').trim(),
        serviceNames: [...document.querySelectorAll('#svc-slot h3 a')].map((a) => a.textContent.trim()).sort(),
        files: [...document.querySelectorAll('#files-slot td')].map((t) => t.textContent.trim()).sort(),
        filesSummary: document.querySelector('#files-slot summary').textContent.replace(/\s+/g, ' ').trim() }));
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
      expect.soft(drawn.filesSummary, 'the Files fold').not.toMatch(/\(\?\)/);
      expect(drawn.files, 'the Files fold through the door lists exactly the files the store holds').toEqual(held);
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
