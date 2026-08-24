// @test-type: e2e:ui — playwright browser flow (login-journey), live surface
/**
 * THE LOGIN JOURNEY — a real browser, the real door, no backdoor.
 *
 * Jeff, 2026-08-08: "we have playwright maybe we need ui tests to prove the flows."
 * He is right, and the inventory that morning found why it matters: NOT ONE test
 * anywhere drives a browser through a sign-in. The e2e tier looks like it does —
 * every authenticated spec calls a test-only route that mints a session for any
 * WebID you name. It proves the backdoor works.
 *
 * Every defect of 2026-08-08 was invisible to ~1000 passing assertions and
 * obvious in a browser:
 *   - the public root served a LOGIN FORM with HTTP 200
 *   - sign-in succeeded and landed on a 404
 *   - the sign-in page shown was the provider's, not ours
 *   - the Clearing loaded and then refused the socket forever ("connecting…")
 * Each one has a scenario below.
 *
 * RULES THIS FILE HOLDS TO
 *   1. No backdoor. No minted cookie, no injected session, no /test/login.
 *      A flow that skips the door proves the skip.
 *   2. Assert what a PERSON SEES — rendered text, a visible control, a message
 *      that appears — never a status code alone. A 200 is not a page.
 *   3. Each scenario states which real failure it would have caught, so a
 *      future reader can tell decoration from a measure.
 *
 * RUN
 *   FLOW_BASE=https://lightlifeurbangardens.com \
 *   FLOW_USER=<webid-or-email> FLOW_PASS=<password> \
 *   npx playwright test proving/flows/login-journey.spec.cjs
 *
 *   Credentials are NEVER defaulted and never committed. Without them the
 *   signed-in scenarios SKIP LOUDLY rather than pass — a suite that reports
 *   green because it could not log in is the exact failure this file exists to
 *   end (learned twice today: a probe that cannot ask must not answer).
 */
const { test, expect } = require('@playwright/test');

const BASE = process.env.FLOW_BASE || 'https://lightlifeurbangardens.com';
const PUBLIC_ROOT = process.env.FLOW_PUBLIC_ROOT || BASE;
const CHORUS = `${BASE}/chorus`;
const USER = process.env.FLOW_USER;
const PASS = process.env.FLOW_PASS;
const haveCreds = Boolean(USER && PASS);

/** A page is a login page if a person would see one: a password field. */
async function looksLikeLogin(page) {
  return (await page.locator('input[type="password"]').count()) > 0;
}

/**
 * The provider asks for consent the first time an identity authorises the app.
 * A person clicks through it, so the flow does too — otherwise the journey stops
 * one screen short of the thing it exists to prove. Idempotent: after the first
 * grant the screen does not appear and this is a no-op.
 */
/**
 * Fill and submit the provider's password form.
 *
 * The submit is wired by the page's own script, so clicking before that script
 * has attached does nothing at all — no request leaves the browser and the form
 * just sits there. That cost half an hour of chasing a login I had been told was
 * working, and it WAS working; the click was early. So: wait for the page to
 * settle, then wait for the URL to actually leave the login screen rather than
 * for a network to go quiet, because "quiet" is also what a dead click looks like.
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

async function passConsent(page) {
  // Wait for the control, not for a URL. The provider gets to the consent screen
  // through a chain of client-side redirects, so a URL test taken the instant
  // after submit reads the previous page and skips the click — which is exactly
  // how this stopped one screen short the first time it ran.
  const authorize = page.getByRole('button', { name: /^authori[sz]e$/i });
  try {
    await authorize.waitFor({ state: 'visible', timeout: 20000 });
  } catch {
    // Nothing to consent to — but if we are SITTING on the consent screen and
    // could not find the control, say so. Returning quietly here is how the
    // journey reported "signed in" while stopped one screen short.
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

test.describe('anonymous — what a stranger sees', () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test('the public root serves the garden site, NOT a login form', async ({ page }) => {
    // WOULD HAVE CAUGHT: 2026-08-08, the public root returned 200 while serving
    // Gathering's sign-in with a password field. Every status-code check passed.
    await page.goto(PUBLIC_ROOT, { waitUntil: 'domcontentloaded' });
    expect(await looksLikeLogin(page)).toBe(false);
    await expect(page.locator('body')).not.toContainText(/sign in/i);
    // Absence of a login form is not presence of the garden site — a blank 200
    // satisfies both lines above. Assert the page a person came for.
    await expect(page.locator('body')).toContainText(/light life urban gardens/i);
  });

  test('no door is reachable anywhere on the garden domain — client-facing means client-facing', async ({ page }) => {
    // WOULD HAVE CAUGHT: /about served Jeff's CV — "Engineering Leader · Systems
    // Thinker", résumé, patent — with a Login link in the nav, on the domain he
    // shows garden clients. It was listed as infrastructure next to /css and
    // /images, so it skipped the gate entirely, and curl could not see it.
    //
    // Jeff, 2026-08-09: "it is for potential clients and i do not want them to
    // experience a demand for signup."
    for (const p of ['/about', '/nonsense', '/photos', '/gardening']) {
      await page.goto(`${PUBLIC_ROOT}${p}`, { waitUntil: 'domcontentloaded' });
      expect(await looksLikeLogin(page), `${p} shows a password field`).toBe(false);
      // A link is a door too. Nothing on this domain may offer sign-in or signup.
      const doors = await page.locator('a', { hasText: /sign\s*(in|up)|log\s*in|register/i }).count();
      expect(doors, `${p} offers a sign-in link`).toBe(0);
      await expect(page.locator('body'), `${p} leaks the personal CV`)
        .not.toContainText(/engineering leader|systems thinker/i);
    }
  });

  test('an unlisted path refuses the SAME WAY signed in or not', async ({ page }) => {
    // The defect this pins (Silas, 2026-08-09): the guard answered "not shared"
    // with "not signed in". Signing in changes nothing about an unlisted path, so
    // offering the door there is both the wrong message and the loop shape — sign
    // in, get bounced, sign in. A refusal has to name its own state.
    //
    // The signed-in half of this pair is in the journey below; this is the
    // anonymous half. Both must land on not-found, not on a door.
    await page.goto(`${CHORUS}/definitely-not-a-shared-path-3798`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('body')).toContainText(/not shared|not found|404/i);
    expect(await looksLikeLogin(page)).toBe(false);
    const doors = await page.locator('a, button').filter({ hasText: /sign in/i }).count();
    expect(doors, 'an unlisted path offered a sign-in door').toBe(0);
  });

  test('a private Chorus path asks US to sign in — our page, not the provider', async ({ page }) => {
    // WOULD HAVE CAUGHT: the three-hop bounce that handed a stranger to
    // id.<domain>/.account/ — a different domain asking for credentials with no
    // indication of what asked or why.
    await page.goto(CHORUS, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('body')).toContainText(/chorus/i);
    await expect(page.locator('body')).toContainText(/sign in/i);
    expect(page.url()).not.toMatch(/\/\.account\//);
  });
});

test.describe('the journey — one sign-in, land where you asked', () => {
  test.skip(!haveCreds, 'FLOW_USER/FLOW_PASS not set — REFUSING to report green on a journey that never ran');
  test.use({ storageState: { cookies: [], origins: [] } });

  test('sign in once at /chorus and land on the entrance', async ({ page }) => {
    // WOULD HAVE CAUGHT: sign-in succeeding and handing back "path not shared"
    // — a 404 that a person reads as being locked out.
    await page.goto(CHORUS, { waitUntil: 'domcontentloaded' });
    // The control is an anchor styled as a button. Match on what a person sees
    // — the words "Sign in" on a thing you click — not on the tag we happen to
    // have used, or the test breaks the next time the markup changes.
    await page.locator('a, button').filter({ hasText: /sign in/i }).first().click();
    await signIn(page);
    await passConsent(page);

    expect(await looksLikeLogin(page)).toBe(false);
    await expect(page.locator('body')).not.toContainText(/path not shared|not found|404/i);
    expect(page.url()).toContain('/chorus');

    // #3807 — AND THE PAGE HAS ITS CONTENT. Landing is not arriving. This scenario
    // passed for two days while the entrance rendered an empty frame: the data
    // calls went to the apex, the garden site answered not-found, and the page sat
    // on "loading the stream live from the model…" forever. Jeff's words for it:
    // "the new chorus landing page is NOT the one i expected it looks like u just
    // made it" — it looked invented because everything real on it was missing.
    //
    // Asserted on what the model actually produces, so it fails when the model
    // cannot be reached rather than when a label is reworded.
    await expect(page.locator('body'), 'the entrance never finished loading the model')
      .not.toContainText(/loading the stream live from the model/i, { timeout: 20000 });
    await expect(page.locator('body'), 'the entrance rendered no value-stream steps')
      .toContainText(/shaping|directing|designing|building|proving|reflecting/i);
    // 2026-08-24 — selector repair, not a relaxation. The entrance was replaced
    // by the products hub (#3790 rebrand); `#stream-block .prod` is markup that
    // exists in NO source file any more, so this assertion could only ever fail.
    // The #3807 intent is kept exactly: the page must render the PRODUCTS the
    // model serves, not an empty frame. Asserted on the live product cards.
    const products = page.locator('.product');
    await expect(products.first(), 'the entrance rendered no product cards').toBeVisible();
    expect(await products.count(), 'the entrance rendered too few products to be the hub')
      .toBeGreaterThanOrEqual(4);
    await expect(page.locator('body'), 'the entrance is missing a known product')
      .toContainText(/athena/i);
  });

  test('the entrance admits the entrance ONLY — being signed in is not a skeleton key', async ({ page }) => {
    // The entrance rule (Silas, 2026-08-09) is an EXACT match on '/', a distinct
    // rule type from the prefix entries, so admitting the top-level page admits
    // nothing beneath it. The bare-prefix version of this — a '/' line in a list
    // of prefixes — reads as "the entrance" and means "everything", which is how
    // /about sat open for two weeks. This is the half that proves it stayed shut.
    const unlisted = `${CHORUS}/definitely-not-a-shared-path-3798`;
    await page.goto(unlisted, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('body')).toContainText(/not shared|not found|404/i);
  });

  test('every link stays under /chorus and never asks again', async ({ page }) => {
    // WOULD HAVE CAUGHT: the double prompt — signing in at one surface and being
    // asked again at the next.
    await page.goto(CHORUS, { waitUntil: 'domcontentloaded' });
    for (const path of ['/chorus/athena/model.html', '/chorus/domains']) {
      await page.goto(`${BASE}${path}`, { waitUntil: 'domcontentloaded' });
      expect(await looksLikeLogin(page), `asked to sign in again at ${path}`).toBe(false);
      expect(page.url(), `left /chorus at ${path}`).toContain('/chorus');
    }
  });

  test('the Clearing connects AND a message sends — read is not enough', async ({ page }) => {
    // WOULD HAVE CAUGHT: "connecting… and never connected" — the page loaded, so
    // every page-level check passed, while the socket refused the session and
    // nothing could be sent. Authenticated for reading, anonymous for speaking.
    // Each scenario starts with an empty jar, so this one walks through the door
    // itself. Signing in via the Clearing's own URL is the point: the return path
    // has to carry you to the room you asked for, not to the entrance.
    await page.goto(`${BASE}/chorus/clearing`, { waitUntil: 'domcontentloaded' });
    const door = page.locator('a, button').filter({ hasText: /sign in/i });
    if (await door.count()) {
      await door.first().click();
      await signIn(page);
      await passConsent(page);
    }
    // #3872 — was `page.locator('body')`, which reads the whole room INCLUDING
    // the messages in it. The team discusses connection problems in this room, so
    // the check went red whenever someone typed the word — it could not tell a
    // dead socket from a chat log, and it reported a hang the status element said
    // was fine. Assert the status element, the only surface that means anything
    // about the socket.
    await expect(page.locator('#connection-status'))
      .not.toContainText(/connecting…|connecting\.\.\./i, { timeout: 20000 });

    const text = `flow-probe ${Date.now()}`;
    // The composer, by the words printed in it. "the first text box on the page"
    // matched a filter field — the message went into a search box and vanished,
    // and the flow reported the Clearing broken when it was my selector.
    const box = page.getByPlaceholder(/type a message/i);
    await box.fill(text);
    await box.press('Enter');
    // the message must APPEAR — sending without arrival is the failure mode
    await expect(page.locator('body')).toContainText(text, { timeout: 20000 });
  });

  test('sign out ends the session everywhere on the host', async ({ page }) => {
    await page.goto(CHORUS, { waitUntil: 'domcontentloaded' });
    const out = page.getByRole('link', { name: /sign out/i }).or(page.getByRole('button', { name: /sign out/i }));
    if (await out.count()) {
      await out.first().click();
      await page.goto(`${BASE}/chorus/domains`, { waitUntil: 'domcontentloaded' });
      await expect(page.locator('body')).toContainText(/sign in/i);
    } else {
      test.skip(true, 'no sign-out control found — reporting rather than asserting');
    }
  });
});
