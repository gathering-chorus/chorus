// @test-type: e2e:ui — playwright: the principles pages opened through the real sign-in door, at both public mounts
// @domain: tests
/**
 * #5040 — Jeff, 2026-11-25 08:06, five phone screenshots from the car: "the page is way too broken
 * to do an interview". At that minute all 13 browser tests of #5034 were green. They open the
 * pages at localhost:3340, at the root, with no door. He opens them at the public address:
 *   - lightlifeurbangardens.com/chorus/…   the door mounted under /chorus
 *   - chorus.lightlifeurbangardens.com/…   the door at the root of its own host
 * Both are behind chorus-share-guard: an allow list, a session, GET and HEAD only.
 *
 * This suite stands where he stands. lib/public-door-5040.cjs starts the real door on a scratch
 * port for each mount, signs a test person in with the door's own code, and opens the three
 * principles pages at phone width. What must hold, at BOTH mounts:
 *   AC1  every fold is filled or shows its true reason: none says could not read, cannot reach,
 *        fetch failure, an HTTP error or "no answer"; the model view draws; the rows page lists
 *        every row the model holds;
 *   AC2  nothing the page asks for leaves the door, the door refuses nothing the page asks for,
 *        and every link drawn stays inside the door and leads somewhere;
 *   AC3  the Services and Files folds read through the door (a GET), and show the same as on
 *        localhost. The door stays GET/HEAD only: no page POSTs.
 * NEGATIVE PROOFS: the page as it was on main (cut from git at the commit before this card) is
 * red under /chorus naming the folds; a planted link that skips the mount is named; a page that
 * POSTs is named.
 *
 * Page code this suite opens and reads, named so a change to any of it runs this suite:
 *   platform/api/public/athena/domain.html, platform/api/public/athena/domains-view.html,
 *   platform/api/public/athena/facets.js, platform/api/public/athena/model-fold.js,
 *   platform/api/public/athena/rows.js, platform/api/public/athena/athena-flow.js.
 *
 * Named limit: the door's upstream is the live chorus-api, so an endpoint change is not tested
 * here before it lands; and a phone's slow connection is held only by the read budget below.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-public-door.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { execFileSync } = require('child_process');
const { startDoor, signedInPage, DOMAIN, BASE } = require('./lib/public-door-5040.cjs');
const { ROOT } = require('./lib/athena-page-5034.cjs');

const MOUNTS = [['under /chorus (the public apex)', '/chorus'], ['at the root (the door\'s own host)', '']];
const NO_ANSWER = /could not read|cannot reach|cannot measure|cannot read|fetch failure|no answer|load failed|HTTP \d{3}|loading the materialized view|loading live from the model|running the domain lint/i;
// one read the page makes may take this long through the door before the fold is called slow:
// a phone connection gave up on the 14-19 s whole-table read (Jeff's Code fold, 08:16)
const READ_BUDGET_MS = 5_000;

/** Open a page through the door and wait until nothing on it is still loading. */
async function open(page, door, pathAndQuery) {
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

async function graphCount(request, kind) {
  const r = await request.post(`${BASE}/sparql-read`, { timeout: 30_000, headers: { Accept: 'application/sparql-results+json' },
    data: { query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a chorus:${kind} } }` } });
  if (!r.ok()) throw new Error(`the store did not answer for ${kind}: HTTP ${r.status()}`);
  return Number((await r.json()).results.bindings[0].n.value);
}

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
      await open(page, door, `/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`);
      const broken = await page.evaluate(brokenFolds, NO_ANSWER.source);
      expect.soft(broken, `folds that failed through the door:\n${broken.join('\n')}`).toEqual([]);
      expect.soft([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      // the policy lint runs a real measurement and is allowed its time; every other read is held to the budget
      const tooSlow = slow.filter((s) => !/\/api\/chorus\/policies\/counts/.test(s));
      expect.soft(tooSlow, `reads over ${READ_BUDGET_MS / 1000} s, which a phone connection gives up on:\n${tooSlow.join('\n')}`).toEqual([]);
      const folds = await page.evaluate(() => document.querySelectorAll('details.fold').length);
      expect(folds, 'the page drew its folds').toBeGreaterThan(12);
      await context.close();
    });

    test('the model view draws the schema and a card for every row, through the door', async ({ browser, request }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      await open(page, door, `/domains/${encodeURIComponent(DOMAIN)}`);
      const broken = await page.evaluate(brokenFolds, NO_ANSWER.source);
      expect.soft(broken, `the model view failed through the door:\n${broken.join('\n')}`).toEqual([]);
      expect.soft([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      const held = await graphCount(request, 'Principle');
      expect(await page.locator('[data-mf="instances"] a.mfcard').count(), `the model holds ${held} Principle rows`).toBe(held);
      await context.close();
    });

    test('the rows page lists every row the model holds, through the door', async ({ browser, request }) => {
      const { page, context, refused } = await signedInPage(browser, door);
      await open(page, door, '/athena/rows.html?kind=Principle');
      const held = await graphCount(request, 'Principle');
      expect.soft(await page.locator('#main table.rows tbody tr').count(), `the model holds ${held} rows`).toBe(held);
      expect([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      await context.close();
    });

    test('every link on the ops view and the model view stays inside the door and leads somewhere', async ({ browser }) => {
      const { page, context } = await signedInPage(browser, door);
      const dead = [];
      for (const p of [`/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, `/domains/${encodeURIComponent(DOMAIN)}`]) {
        await open(page, door, p);
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

    test('the Services and Files folds show through the door what localhost shows, with no POST', async ({ browser, request }) => {
      const read = async (origin, prefix, withDoor) => {
        const made = withDoor ? await signedInPage(browser, door) : null;
        const context = made ? made.context : await browser.newContext({ viewport: { width: 390, height: 844 } });
        const page = made ? made.page : await context.newPage();
        const posts = [];
        page.on('request', (r) => { if (r.method() !== 'GET' && r.method() !== 'HEAD') posts.push(`${r.method()} ${r.url().replace(origin, '')}`); });
        await page.goto(`${origin}${prefix}/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => document.querySelector('#svc-slot details') && document.querySelector('#files-slot details'), null, { timeout: 120_000 });
        const drawn = await page.evaluate(() => ({ services: document.querySelector('#svc-slot').textContent.replace(/\s+/g, ' ').trim(), files: [...document.querySelectorAll('#files-slot td')].map((t) => t.textContent.trim()).sort(), filesSummary: document.querySelector('#files-slot summary').textContent.replace(/\s+/g, ' ').trim() }));
        await context.close();
        return { drawn, posts };
      };
      const through = await read(door.origin, door.mount, true);
      expect.soft(through.posts, `the page must only read (GET) through the door:\n${through.posts.join('\n')}`).toEqual([]);
      expect.soft(through.drawn.services, 'the Services fold').not.toMatch(NO_ANSWER);
      expect.soft(through.drawn.filesSummary, 'the Files fold').not.toMatch(/\(\?\)/);
      // the truth is the graph's own answer, asked directly on localhost (the query the fold used to POST)
      const r = await request.post(`${BASE}/sparql-read`, { timeout: 30_000, headers: { Accept: 'application/sparql-results+json' },
        data: { query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT DISTINCT ?path WHERE { GRAPH ?g { ?f a chorus:CodeFile ; chorus:hasDomain <https://jeffbridwell.com/chorus#${DOMAIN}> ; chorus:filePath ?path } }` } });
      expect(r.ok(), 'the store answers the files query').toBe(true);
      const held = (await r.json()).results.bindings.map((b) => String(b.path.value)).sort();
      expect(held.length, 'the graph holds files for this domain').toBeGreaterThan(0);
      expect(through.drawn.files, 'the Files fold through the door lists exactly the files the graph holds').toEqual(held);
    });
  });
}

test.describe('#5040 NEGATIVE: the check is red on the pages that broke on Jeff\'s phone', () => {
  test.setTimeout(300_000);
  test('the ops view as it was before this card, under /chorus, is named fold by fold', async ({ browser }) => {
    // the page files as they stood at the commit before #5040 touched them
    const before = (f) => execFileSync('git', ['-C', ROOT, 'show', `112388152:platform/api/public/athena/${f}`], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
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
      const broken = await page.evaluate(brokenFolds, NO_ANSWER.source);
      for (const fold of ['Conformance', 'Services', 'Files', 'Cards', 'Code']) expect(broken.join('\n'), `the old page must be caught on its ${fold} fold`).toMatch(new RegExp(`^${fold}`, 'm'));
      expect(refused.join('\n'), 'the old page asks outside the door').toMatch(/LEFT THE DOOR GET \/api\/chorus\/domain\//);
      expect(refused.join('\n'), 'the old page POSTs, which the door refuses').toMatch(/405 POST \/chorus\/sparql-read/);
      await context.close();
    } finally { door.stop(); }
  });

  test('a link that skips the mount is named, and a link inside it is not', async ({ browser }) => {
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
