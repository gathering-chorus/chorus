// @domain: tests — shared by the #5040 public-door suite
/**
 * #5040 — open the Athena pages the way Jeff's phone does: through the sign-in door.
 *
 * Jeff, 2026-11-25 08:06, with five phone screenshots: "the page is way too broken to do an
 * interview". All 13 browser tests of #5034 were green at that moment, because they open the
 * pages at localhost:3340, at the root. His phone opens them at the public address, where the
 * pages sit under /chorus behind chorus-share-guard: GET and HEAD only, an allow list, a session.
 *
 * This starts the REAL door (platform/scripts/chorus-share-guard.py) on a scratch port:
 *   - mounted at /chorus (SHARE_PATH_PREFIX), as the public apex mounts it;
 *   - the real allow list (config/share-allowlist.txt), read from THIS tree;
 *   - its upstream is the live chorus-api, as in production;
 *   - a scratch identity world (one test person, a scratch cookie key, offline: no identity
 *     provider is contacted), and a session minted by the door's own --sign-session.
 * Nothing of the door is re-implemented here. The pages' own files are served from this tree
 * (lib/athena-page-5034.cjs explains why); every data call goes through the door.
 */
const fs = require('fs');
const os = require('os');
const net = require('net');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const { ROOT, ATHENA_DIR, BASE, DOMAIN, PHONE, ownAthenaFile } = require('./athena-page-5034.cjs');

const GUARD = path.join(ROOT, 'platform', 'scripts', 'chorus-share-guard.py');
const ALLOW = path.join(ROOT, 'config', 'share-allowlist.txt');
const MOUNT = '/chorus';
const PERSON = 'https://example.test/tester-5040#me';
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8' };

const freePort = () => new Promise((resolve, reject) => {
  const s = net.createServer();
  s.once('error', reject);
  s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => resolve(port)); });
});

/**
 * Start the door. Returns { origin, mount, cookie, stop, log }.
 * `mount` is where the door sits: '/chorus' on the public apex, '' on the door's own host
 * (chorus.<site>). `allowFile` lets a negative proof run the door on another list.
 */
async function startDoor({ allowFile = ALLOW, mount = MOUNT } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'door-5040-'));
  const port = await freePort();
  fs.writeFileSync(path.join(dir, 'principals.txt'), PERSON + '\n');
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
  const stop = () => { try { child.kill(); } catch { /* already gone */ } fs.rmSync(dir, { recursive: true, force: true }); };
  const log = () => { try { return fs.readFileSync(logFile, 'utf8'); } catch { return ''; } };
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) { const text = log(); stop(); throw new Error(`the door did not start (exit ${child.exitCode}):\n${text}`); }
    try { await fetch(`${origin}${mount}/_auth/whoami`, { redirect: 'manual' }); break; } catch { await new Promise((r) => setTimeout(r, 100)); }
    if (i === 99) { const text = log(); stop(); throw new Error(`the door never answered on ${origin}:\n${text}`); }
  }
  // the session is signed by the door's own code, with the key the running door just wrote
  const cookie = execFileSync('python3', [GUARD, '--sign-session', PERSON, 'tester-5040-browser'], { env, encoding: 'utf8' }).trim();
  return { origin, mount, cookie, stop, log };
}

/** A browser page signed in at the door, at phone width, with this tree's own page files. */
async function signedInPage(browser, door, { width = PHONE.width, height = PHONE.height } = {}) {
  const context = await browser.newContext({ viewport: { width, height } });
  await context.addCookies([{ name: 'chorus_share_session', value: door.cookie, url: door.origin }]);
  const page = await context.newPage();
  const refused = [];
  const M = door.mount;
  const esc = M.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  // every request the page makes that the door (or anything else) answers 4xx/5xx, and every one that leaves the door
  page.on('response', (r) => { if (r.status() >= 400) refused.push(`${r.status()} ${r.request().method()} ${r.url().replace(door.origin, '')}`); });
  page.on('request', (r) => { const u = r.url(); if (/^https?:/.test(u) && !u.startsWith(door.origin + M + '/') && !/fonts\.(googleapis|gstatic)\.com/.test(u)) refused.push(`LEFT THE DOOR ${r.method()} ${u.replace(door.origin, '')}`); });
  page.on('requestfailed', (r) => refused.push(`FAILED ${r.method()} ${r.url().replace(door.origin, '')} (${r.failure()?.errorText || 'no reason'})`));
  // this tree's page files, under the mount: /chorus/athena/<file>, and the one model-view template
  await page.route((url) => url.origin === door.origin && new RegExp(`^${esc}/athena/[^?#]+\\.(html|js|css)$`).test(url.pathname), (route) => {
    const file = ownAthenaFile(route.request().url());
    if (!file) return route.fulfill({ status: 404, contentType: 'text/plain', body: 'not in this tree' });
    return route.fulfill({ status: 200, contentType: TYPES[path.extname(file)] || 'application/octet-stream', body: fs.readFileSync(file) });
  });
  await page.route((url) => url.origin === door.origin && (url.pathname === `${M}/domains` || url.pathname.startsWith(`${M}/domains/`)), (route) =>
    route.fulfill({ status: 200, contentType: TYPES['.html'], body: fs.readFileSync(path.join(ATHENA_DIR, 'domains-view.html')) }));
  return { page, context, refused };
}

module.exports = { startDoor, signedInPage, MOUNT, ALLOW, GUARD, DOMAIN, BASE, PHONE };
