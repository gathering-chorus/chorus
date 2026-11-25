// @domain: tests — shared by the #5034 Athena domain-page suites
/**
 * #5034 — how the Athena domain-page suites open the page.
 *
 * The pipeline's test step runs before the card is deployed anywhere, and the only server
 * with Athena's data is the live one, which serves main's copy of the page (Kade, 11-24).
 * So the page's own files (platform/api/public/athena/*) are served from THIS tree with
 * page.route, and only the data calls (/owl, /api, /sparql-read, /domains…) go to the live
 * server. A card that changes the page is graded on its own page.
 * Named limit: a change to an endpoint is not tested here before it lands.
 */
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..', '..');
const ATHENA_DIR = path.join(ROOT, 'platform', 'api', 'public', 'athena');
const BASE = (process.env.ATHENA_URL || 'http://localhost:3340').replace(/\/+$/, '');
const DOMAIN = process.env.ATHENA_DOMAIN || 'principles';
const PHONE = { width: 390, height: 844 };
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8' };

/**
 * The file in this tree's athena directory a request URL names, or null. Null for a name that
 * leaves the directory (an encoded `..`, an absolute path) and for a file the tree does not have.
 */
function ownAthenaFile(url) {
  const name = decodeURIComponent(new URL(url).pathname.split('/athena/')[1] || '');
  const file = path.join(ATHENA_DIR, name);
  return file.startsWith(ATHENA_DIR + path.sep) && fs.existsSync(file) ? file : null;
}

/** Serve /athena/<file> from this tree. A file this tree does not have is a 404, never the live copy. */
async function serveOwnAthenaFiles(page) {
  await page.route(/\/athena\/[^?#]+\.(html|js|css)(\?.*)?$/, async (route) => {
    const file = ownAthenaFile(route.request().url());
    if (!file) {
      const name = new URL(route.request().url()).pathname.split('/athena/')[1] || '';
      return route.fulfill({ status: 404, contentType: 'text/plain', body: `not in this tree: athena/${name}` });
    }
    return route.fulfill({ status: 200, contentType: TYPES[path.extname(file)] || 'application/octet-stream', body: fs.readFileSync(file) });
  });
}

/** Open the domain page at the given width and wait until its folds have drawn. */
async function openDomainPage(page, { width = PHONE.width, height = PHONE.height, domain = DOMAIN } = {}) {
  await page.setViewportSize({ width, height });
  await serveOwnAthenaFiles(page);
  await serveDiscoveryAsLanded(page);
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`${BASE}/athena/domain.html?d=${encodeURIComponent(domain)}`, { waitUntil: 'domcontentloaded' });
  // the facets and the files fold draw after their own fetches; the placeholder text goes when they have
  await page.waitForFunction(() => {
    const f = document.getElementById('facets');
    const files = document.querySelector('#files-slot details');
    return f && files && f.querySelector('details.fold') && !/loading the materialized view/.test(f.textContent);
  }, null, { timeout: 45_000 });
  // a closed fold still holds its table: open every one, as a reader would
  await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
  return { errors };
}

const isModelViewPath = (p) => p === '/domains' || p.startsWith('/domains/');

/**
 * Discovery as it will read once this card's server change is live.
 * The directory of what is served (GET /owl/) is built by chorus-api, and the pipeline's test
 * step runs before any server from this tree exists (Kade, 11-24). So before the land the live
 * directory still lacks every domain served by its own generated API, and the page would be
 * graded on the old server. This adds the entries that are MISSING, in the merged form the
 * server change produces (the API's own route with /v1 in front); an entry the live directory
 * already has is left alone. ATHENA_DISCOVERY=live turns this off, and so does the nightly
 * (werk-test sets WERK_TEST_NIGHTLY=1, #5034 AC5): the nightly and the after-the-land check
 * read the directory exactly as served.
 * `omit` removes kinds instead: a negative proof draws the page as it was before the fix.
 */
// Nothing is stood in any more. The server change landed 11-24 20:22 and the live directory lists
// Principle itself, so the suites read it as served. The stand-in that was here wrote its entry
// with "kind" before "collection"; the real answer has them the other way round, and rows.js
// matched on that order, so the rows page passed here and was wrong in production (found by the
// after-the-land run, 20:29). A stand-in for a server change must be the server's real answer,
// byte for byte, or not exist.
const LANDED_ENTRIES = [];
const LIVE_DISCOVERY = process.env.ATHENA_DISCOVERY === 'live' || process.env.WERK_TEST_NIGHTLY === '1';
function asLanded(body) {
  if (LIVE_DISCOVERY) return body;
  for (const p of LANDED_ENTRIES) if (!(body.primitives || []).some((x) => x.kind === p.kind)) (body.primitives ||= []).push(p);
  return body;
}
async function serveDiscoveryAsLanded(page, omit = []) {
  if (LIVE_DISCOVERY && !omit.length) return;
  await page.route((url) => url.origin === new URL(BASE).origin && url.pathname === '/owl/', async (route) => {
    const live = await route.fetch();
    const body = asLanded(await live.json());
    body.primitives = (body.primitives || []).filter((p) => !omit.includes(p.kind));
    return route.fulfill({ response: live, json: body });
  });
}

/**
 * What the doors themselves hold for each class the domain declares, asked directly and never
 * through the page's own lookup (Kade, 11-24: a test that reuses the page's logic shares its bug).
 * Returns [{ kind, held, collection, rows }]:
 *   held        how many rows of the class the GRAPH holds, counted by a read of the store. This
 *               is the truth the page is held to. It does not pass through any directory of what
 *               is served, so a directory that leaves a class out cannot make the page look right
 *               (first draft, 11-24 14:57: truth came from the directory, and the test agreed
 *               with "Not served" over 14 rows).
 *   collection  where the directory says the class is served; null when it lists none.
 *   rows        the row names that collection answers with; null when it did not answer.
 */
async function graphCount(page, kind) {
  const r = await page.request.post(`${BASE}/sparql-read`, { timeout: 30_000, headers: { Accept: 'application/sparql-results+json' },
    data: { query: `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a chorus:${kind.replace(/[^\w-]/g, '')} } }` } });
  if (!r.ok()) throw new Error(`the store did not answer for ${kind}: HTTP ${r.status()}`);
  return Number((await r.json()).results.bindings[0].n.value);
}
async function doorTruth(page, domain = DOMAIN) {
  const get = async (p) => { const r = await page.request.get(BASE + p, { timeout: 20_000 }); return r.ok() ? r.json() : null; };
  const row = ((await get('/owl/domains'))?.data || []).find((r) => r.name === domain);
  const vocab = [].concat(row?.definesVocabulary || []).filter(Boolean);
  const listed = asLanded((await get('/owl/')) || {}).primitives || [];
  const out = [];
  for (const kind of vocab) {
    const collection = listed.find((p) => p.kind === kind)?.collection || null;
    const body = collection ? await get('/owl' + collection) : null;
    out.push({ kind, held: await graphCount(page, kind), collection, rows: body && Array.isArray(body.data) ? body.data.map((r) => String(r.name)).sort() : null });
  }
  return out;
}

/** Serve the model view (/domains, /domains/<d>, /domains/<d>/<i>) from this tree's one template. */
async function serveOwnModelView(page) {
  await page.route((url) => url.origin === new URL(BASE).origin && isModelViewPath(url.pathname), (route) =>
    route.fulfill({ status: 200, contentType: TYPES['.html'], body: fs.readFileSync(path.join(ATHENA_DIR, 'domains-view.html')) }));
}

/** Open the model view of the domain and wait until its shared fold has drawn schema and instances. */
async function openModelView(page, { width = PHONE.width, height = PHONE.height, domain = DOMAIN, omit = [] } = {}) {
  await page.setViewportSize({ width, height });
  await serveOwnAthenaFiles(page);
  await serveOwnModelView(page);
  await serveDiscoveryAsLanded(page, omit);
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(`${BASE}/domains/${encodeURIComponent(domain)}`, { waitUntil: 'domcontentloaded' });
  await page.waitForSelector('#mfold-slot[data-mfold-ready="1"]', { timeout: 45_000 });
  return { errors };
}

/** Every same-origin link the page shows, once each. */
async function sameOriginLinks(page) {
  return page.evaluate(() => [...new Set([...document.querySelectorAll('a[href]')].map((a) => a.href))]
    .filter((h) => h.startsWith(location.origin + '/')));
}

/**
 * Why a link is dead, or null when it leads somewhere.
 *   - a page of the Athena family must exist in this tree;
 *   - anything else must answer under 400;
 *   - a model-view path is one template that answers 200 for ANY name, so its status proves
 *     nothing: it is opened, and a page that drew its "no such…/not found/could not read"
 *     note is dead.
 */
async function deadLinkReason(page, href) {
  const u = new URL(href);
  if (/^\/athena\/[^/]+\.(html|js|css)$/.test(u.pathname)) {
    return fs.existsSync(path.join(ATHENA_DIR, decodeURIComponent(u.pathname.slice('/athena/'.length)))) ? null : 'no such Athena page in this tree';
  }
  if (isModelViewPath(u.pathname)) {
    const tab = await page.context().newPage();
    try {
      await serveOwnAthenaFiles(tab);
      await serveOwnModelView(tab);
      await serveDiscoveryAsLanded(tab);
      await tab.goto(href, { waitUntil: 'domcontentloaded' });
      await tab.waitForFunction(() => !/Loading the model/.test(document.getElementById('app')?.textContent || ''), null, { timeout: 30_000 });
      const note = await tab.evaluate(() => document.querySelector('#app > .note')?.textContent.trim().slice(0, 90) || null);
      return note ? `the page says: ${note}` : null;
    } catch (e) {
      return `did not draw: ${String(e.message || e).split('\n')[0]}`;
    } finally { await tab.close(); }
  }
  try {
    const r = await page.request.get(href, { timeout: 20_000 });
    return r.status() < 400 ? null : `HTTP ${r.status()}`;
  } catch (e) { return `no answer: ${String(e.message || e).split('\n')[0]}`; }
}

/** The dead links among `hrefs`, as "path — reason" lines. */
async function deadLinks(page, hrefs) {
  const dead = [];
  for (const h of hrefs) {
    const why = await deadLinkReason(page, h);
    if (why) dead.push(`${h.replace(BASE, '')} — ${why}`);
  }
  return dead;
}

module.exports = { LIVE_DISCOVERY, ROOT, ATHENA_DIR, BASE, DOMAIN, PHONE, ownAthenaFile, serveOwnAthenaFiles, serveOwnModelView, serveDiscoveryAsLanded, openDomainPage, openModelView, sameOriginLinks, deadLinks, doorTruth };
