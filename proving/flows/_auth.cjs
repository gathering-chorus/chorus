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
const USER = process.env.FLOW_USER;
const PASS = process.env.FLOW_PASS;
const haveCreds = Boolean(USER && PASS);

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
  await page.waitForLoadState('networkidle');
  const password = page.locator('input[type="password"]').first();
  await password.waitFor({ state: 'visible' });
  await page.locator('input[type="email"], input[name="email"], input[name="username"]').first().fill(USER);
  await password.fill(PASS);
  const loginUrl = page.url();
  await page.locator('button[type="submit"], input[type="submit"]').first().click();
  await page.waitForURL((u) => u.toString() !== loginUrl, { timeout: 30000 });
  await page.waitForLoadState('networkidle');
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
  await page.waitForLoadState('networkidle');
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

module.exports = { haveCreds, looksLikeLogin, signIn, passConsent, gotoSignedIn };
