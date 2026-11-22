// Shared sign-in for the browser flows (#2725, 2026-08-24).
//
// Extracted from login-journey.spec.cjs, unchanged in behaviour, because a
// SECOND spec (chorus-home-3886) needs the same journey: its "is there a way
// in" assertions target the /chorus ENTRANCE, which anonymous cannot see — the
// public surface answers with our own sign-in door. Run anonymous, those tests
// fail as if the links were missing; that red says "dead end" when the truth is
// "not signed in", and the two states must not be told apart by guessing.
//
// Credentials come from the environment only (FLOW_USER / FLOW_PASS, loaded by
// the playwright config from the machine's creds file). Nothing here reads,
// prints, or persists a secret; when the env is empty the caller SKIPS with a
// reason rather than reporting green on a journey that never ran.
// #4915 — the loader lives here, so a script outside the playwright runner (the room
// check, platform/tests/lib/room-rows-4887.cjs) signs in the same way; the playwright
// config calls it. Env vars already set win over the file. Nothing is printed.
const fs = require('fs');
const os = require('os');
const path = require('path');
function loadFlowCreds() {
  const credFile = process.env.FLOW_CREDS_FILE || path.join(os.homedir(), '.chorus', 'flow-creds.env');
  if (!fs.existsSync(credFile)) return;
  for (const line of fs.readFileSync(credFile, 'utf8').split('\n')) {
    const m = line.match(/^\s*(?:export\s+)?([A-Z0-9_]+)\s*=\s*(.*)$/);
    if (!m) continue;
    if (process.env[m[1]] === undefined) process.env[m[1]] = m[2].trim().replace(/^['"]|['"]$/g, '');
  }
}
loadFlowCreds();
const USER = process.env.FLOW_USER;
const PASS = process.env.FLOW_PASS;
const haveCreds = Boolean(USER && PASS);

// #5026 AC2 — every walk ends the way a person's does: signed out. Since #5015
// each probe sign-in opens a Session row at the door, and a walk that never
// signed out left it open (50 open rows by 11-21, failing the open-session
// rule). signIn remembers EVERY page it signed in (the default one or one a
// spec opened itself); the `test` exported below signs them all out when the
// test ends, pass or fail, so no spec has to remember to.
const signedIn = new Set();

/** A page is a login page if a person would see one: a password field. */
async function looksLikeLogin(page) {
  return (await page.locator('input[type="password"]').count()) > 0;
}

/**
 * Fill and submit the provider's password form.
 *
 * The submit is wired by the page's own script, so clicking before that script
 * has attached does nothing at all. Wait for the page to settle, then wait for
 * the URL to leave the login screen — "network quiet" is also what a dead click
 * looks like.
 */
async function signIn(page) {
  // #4915 — 'load', never 'networkidle': a page with a live stream (the Clearing room, its
  // socket) is never idle, and the room check timed out here at 30 s. The password field's
  // own wait below is what says the form is ready.
  await page.waitForLoadState('load');
  const password = page.locator('input[type="password"]').first();
  await password.waitFor({ state: 'visible' });
  await page.locator('input[type="email"], input[name="email"], input[name="username"]').first().fill(USER);
  await password.fill(PASS);
  const loginUrl = page.url();
  await page.locator('button[type="submit"], input[type="submit"]').first().click();
  await page.waitForURL((u) => u.toString() !== loginUrl, { timeout: 30000 });
  await page.waitForLoadState('load');
  signedIn.add(page);
}

/**
 * The provider asks for consent the first time an identity authorises the app.
 * Idempotent: after the first grant the screen does not appear.
 */
async function passConsent(page) {
  const authorize = page.getByRole('button', { name: /^authori[sz]e$/i });
  try {
    await authorize.waitFor({ state: 'visible', timeout: 20000 });
  } catch {
    if (/\/consent/.test(page.url())) {
      throw new Error(`on the consent screen with no Authorize control: ${page.url()}`);
    }
    return;
  }
  const before = page.url();
  await authorize.click();
  await page.waitForURL((u) => u.toString() !== before, { timeout: 30000 });
  await page.waitForLoadState('load');   // #4915 — the landing page may hold a live stream
}

/**
 * Land on `url` signed in. Clicks our own sign-in door when it appears, so a
 * caller writes one line and gets the entrance a person would see.
 */
async function gotoSignedIn(page, url) {
  await page.goto(url, { waitUntil: 'domcontentloaded' });
  const door = page.locator('a, button').filter({ hasText: /sign in/i }).first();
  if (await door.count()) {
    await door.click();
    await signIn(page);
    await passConsent(page);
  }
}

/** The guard's sign-out, in the visitor's frame: <base>/chorus/_auth/logout. */
function logoutUrl(base) {
  return `${String(base).replace(/\/+$/, '')}/chorus/_auth/logout`;
}

/** A spec that signed `page` out itself says so, and the walk's end leaves it alone. */
function markSignedOut(page) {
  signedIn.delete(page);
}

/** Sign `page` out if this walk signed it in; returns whether it did. */
async function signOutIfSignedIn(page, base) {
  if (!signedIn.has(page)) return false;
  signedIn.delete(page);
  await page.goto(logoutUrl(base), { waitUntil: 'domcontentloaded' });
  return true;
}

/**
 * The walk's end: sign out every page this walk signed in and that is still
 * signed in. Never throws: this runs after the test's own verdict, and a
 * sign-out that fails must not turn a passing test red. It says so instead
 * (the door's close-at-sign-in ends such a row at the next sign-in, #4966).
 * Returns how many it signed out.
 */
async function signOutAll(base, warn = console.warn) {
  let done = 0;
  for (const page of [...signedIn]) {
    try {
      if (page.isClosed && page.isClosed()) {
        signedIn.delete(page);
        warn(`sign-out skipped: a signed-in page was closed before the walk ended; its session stays open until the next sign-in`);
        continue;
      }
      if (await signOutIfSignedIn(page, base)) done += 1;
    } catch (e) {
      warn(`sign-out failed, session left open until the next sign-in: ${e && e.message ? e.message : e}`);
    }
  }
  return done;
}

// Playwright's test plus one automatic fixture: after each test, every page
// this walk signed in is signed out. It names `page` so it is torn down while
// the pages are still open. Loaded on first use so the helpers above run (and
// are tested) without the playwright package.
let extended;
function signedOutTest() {
  if (!extended) {
    extended = require('@playwright/test').test.extend({
      signOutAfter: [async ({ page, baseURL }, use) => {
        await use();
        await signOutAll(process.env.FLOW_BASE || baseURL);
      }, { auto: true }],
    });
  }
  return extended;
}

module.exports = {
  loadFlowCreds, haveCreds, looksLikeLogin, signIn, passConsent, gotoSignedIn,
  signedIn, logoutUrl, markSignedOut, signOutIfSignedIn, signOutAll,
  get test() { return signedOutTest(); },
};
