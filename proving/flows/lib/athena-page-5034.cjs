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
  // the facets and the Code fold draw after their own fetches; the placeholder text goes when they have
  // (#5041: the files are in the Code fold, #code-slot; the separate Files fold is gone)
  await page.waitForFunction(() => {
    const f = document.getElementById('facets');
    const code = document.querySelector('#code-slot details');
    return f && code && f.querySelector('details.fold') && !/loading the materialized view/.test(f.textContent);
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

// ---------------------------------------------------------------------------------------------
// #5040 — opening the pages the way Jeff's phone does: through the sign-in door.
//
// Jeff, 2026-11-25 08:06, with five phone screenshots: "the page is way too broken to do an
// interview". Every suite above was green at that moment, because everything above opens the
// pages at localhost:3340, at the root. His phone opens them at the public address, behind
// chorus-share-guard: an allow list, a session, GET and HEAD only, and on the apex a /chorus mount.
//
// startDoor runs the REAL door (platform/scripts/chorus-share-guard.py) on a scratch port:
//   - at the mount asked for: '/chorus' as on the public apex, '' as on the door's own host;
//   - the real allow list (config/share-allowlist.txt), read from THIS tree;
//   - its upstream is the live chorus-api, as in production;
//   - a scratch identity world (one test person, a scratch cookie key, offline: no identity
//     provider is contacted), and a session minted by the door's own --sign-session.
// Nothing of the door is re-implemented here. The pages' own files are served from this tree
// (the header of this file says why); every data call goes through the door.
// (These live in this file, not a second lib: the layout rule counts files under proving/flows.)
const os = require('os');
const net = require('net');
const { spawn, execFileSync } = require('child_process');

const GUARD = path.join(ROOT, 'platform', 'scripts', 'chorus-share-guard.py');
const DOOR_ALLOW = path.join(ROOT, 'config', 'share-allowlist.txt');
const DOOR_PERSON = 'https://example.test/tester-5040#me';

// A port the system says is free. Another process could take it before the door binds; the door
// then exits, and startDoor reports that with the door's own log, never a silent hang.
const freePort = () => new Promise((resolve, reject) => {
  const s = net.createServer();
  s.once('error', reject);
  s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => resolve(port)); });
});

/**
 * Start the door. Returns { origin, mount, cookie, stop, log }.
 * `allowFile` lets a negative proof run the door on another list.
 */
async function startDoor({ allowFile = DOOR_ALLOW, mount = '/chorus' } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'door-5040-'));
  const port = await freePort();
  fs.writeFileSync(path.join(dir, 'principals.txt'), DOOR_PERSON + '\n');
  fs.writeFileSync(path.join(dir, 'person-sessions.json'), JSON.stringify({ 'tester-5040-browser': { principal: 'tester-5040', row: { name: 'tester-5040-browser' } } }));
  const env = { ...process.env,
    SHARE_PRINCIPALS_FILE: path.join(dir, 'principals.txt'), SHARE_STATE_FILE: path.join(dir, 'oidc-state.json'),
    SHARE_PERSON_SESSIONS: path.join(dir, 'person-sessions.json'), SHARE_OIDC_OFFLINE: '1',
    ATHENA_MAKE_URL: 'http://127.0.0.1:9', // a renewal never writes a real Session row
    SHARE_ALLOW_FILE: allowFile, SHARE_UPSTREAM: BASE, SHARE_PATH_PREFIX: mount, SHARE_PORT: String(port),
    SHARE_PUBLIC_ORIGIN: `http://127.0.0.1:${port}`, SHARE_COOKIE_DOMAIN: '' };
  delete env.SHARE_ALLOW;
  const logFile = path.join(dir, 'door.log');
  const out = fs.openSync(logFile, 'a');
  const child = spawn('python3', [GUARD], { env, stdio: ['ignore', out, out] });
  const origin = `http://127.0.0.1:${port}`;
  const stop = () => {
    try { child.kill(); } catch (e) { process.stderr.write(`door-5040: could not stop the door: ${e.message}\n`); }
    fs.rmSync(dir, { recursive: true, force: true });
  };
  // the door's log, for a failure message; a log that cannot be read is said so, not hidden
  const log = () => { try { return fs.readFileSync(logFile, 'utf8'); } catch (e) { return `(door log unreadable: ${e.message})`; } };
  let up = false;
  for (let i = 0; i < 100 && !up; i++) {
    if (child.exitCode !== null) { const text = log(); stop(); throw new Error(`the door did not start (exit ${child.exitCode}):\n${text}`); }
    // not yet listening is the expected answer while it starts: wait and ask again, each ask bounded
    up = await fetch(`${origin}${mount}/_auth/whoami`, { redirect: 'manual', signal: AbortSignal.timeout(2_000) }).then(() => true, () => false);
    if (!up) await new Promise((r) => setTimeout(r, 100));
  }
  if (!up) { const text = log(); stop(); throw new Error(`the door never answered on ${origin}:\n${text}`); }
  // the session is signed by the door's own code, with the key the running door just wrote
  const cookie = execFileSync('python3', [GUARD, '--sign-session', DOOR_PERSON, 'tester-5040-browser'], { env, encoding: 'utf8' }).trim();
  return { origin, mount, cookie, stop, log };
}

/**
 * A browser page signed in at the door, at phone width, with this tree's own page files.
 * `timezoneId` (#5041) puts the reader in another time zone, so "shown in local time" can be told
 * apart from "shown as stored": this machine sits in the zone the stored times are written in.
 */
async function signedInPage(browser, door, { width = PHONE.width, height = PHONE.height, timezoneId } = {}) {
  const context = await browser.newContext({ viewport: { width, height }, ...(timezoneId ? { timezoneId } : {}) });
  await context.addCookies([{ name: 'chorus_share_session', value: door.cookie, url: door.origin }]);
  const page = await context.newPage();
  const refused = [];
  const M = door.mount;
  const mountRe = M.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  // every request the page makes that the door (or anything else) answers 4xx/5xx, and every one that leaves the door
  page.on('response', (r) => { if (r.status() >= 400) refused.push(`${r.status()} ${r.request().method()} ${r.url().replace(door.origin, '')}`); });
  page.on('request', (r) => { const u = r.url(); if (/^https?:/.test(u) && !u.startsWith(door.origin + M + '/') && !/fonts\.(googleapis|gstatic)\.com/.test(u)) refused.push(`LEFT THE DOOR ${r.method()} ${u.replace(door.origin, '')}`); });
  page.on('requestfailed', (r) => refused.push(`FAILED ${r.method()} ${r.url().replace(door.origin, '')} (${r.failure()?.errorText || 'no reason'})`));
  // this tree's page files, under the mount, and the one model-view template
  await page.route((url) => url.origin === door.origin && new RegExp(`^${mountRe}/athena/[^?#]+\\.(html|js|css)$`).test(url.pathname), (route) => {
    const file = ownAthenaFile(route.request().url());
    if (!file) return route.fulfill({ status: 404, contentType: 'text/plain', body: 'not in this tree' });
    return route.fulfill({ status: 200, contentType: TYPES[path.extname(file)] || 'application/octet-stream', body: fs.readFileSync(file) });
  });
  await page.route((url) => url.origin === door.origin && (url.pathname === `${M}/domains` || url.pathname.startsWith(`${M}/domains/`)), (route) =>
    route.fulfill({ status: 200, contentType: TYPES['.html'], body: fs.readFileSync(path.join(ATHENA_DIR, 'domains-view.html')) }));
  return { page, context, refused };
}

// ---------------------------------------------------------------------------------------------
// #5041 — the page as Jeff walked it on his phone, 2026-11-25 10:22 to 10:34: main at #5040.
// Every #5041 negative proof draws THAT page (cut from git, never rebuilt by hand) and shows the
// new check red on it. `under` is where the page is opened: BASE, or a door's origin + mount.
const BEFORE_5041 = 'd15c881c5729f75b691a47b864749cb872389a01';
const OLD_PAGE_FILES = ['domain.html', 'facets.js', 'athena-flow.js', 'athena-flow.css', 'model-fold.js'];
const athenaFileAt = (sha, name) => execFileSync('git', ['-C', ROOT, 'show', `${sha}:platform/api/public/athena/${name}`], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
async function serveAthenaFilesAsOf(page, under, sha = BEFORE_5041) {
  const at = new URL(under + '/athena/');
  await page.route((url) => url.origin === at.origin && url.pathname.startsWith(at.pathname) && OLD_PAGE_FILES.includes(url.pathname.slice(at.pathname.length)), (route) => {
    const name = new URL(route.request().url()).pathname.slice(at.pathname.length);
    return route.fulfill({ status: 200, contentType: TYPES[path.extname(name)], body: athenaFileAt(sha, name) });
  });
}
/** Open the ops view as it was before #5041 and wait until every fold it had has drawn its answer. */
async function openOldDomainPage(page, under, { domain = DOMAIN } = {}) {
  await serveAthenaFilesAsOf(page, under);
  await page.goto(`${under}/athena/domain.html?d=${encodeURIComponent(domain)}`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#facets details.fold') && document.querySelector('#svc-slot details') && document.querySelector('#files-slot details')
    && document.querySelector('#mfold-slot[data-mfold-ready="1"]')
    && !/loading the materialized view|loading live from the model|running the domain lint/.test(document.body.innerText), null, { timeout: 180_000 });
}

module.exports = { BEFORE_5041, athenaFileAt, serveAthenaFilesAsOf, openOldDomainPage, LIVE_DISCOVERY, ROOT, ATHENA_DIR, BASE, DOMAIN, PHONE, ownAthenaFile, serveOwnAthenaFiles, serveOwnModelView, serveDiscoveryAsLanded, openDomainPage, openModelView, sameOriginLinks, deadLinks, doorTruth, startDoor, signedInPage };
