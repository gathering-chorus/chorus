// @test-type: e2e:ui — playwright: the chorus pages RENDER their data in a browser
/**
 * #1778 — page BEHAVIOR, not API behavior. The API-seam suite proves the
 * backing fold serves values; this proves a browser turns them into pixels.
 * Jeff's catch, 2026-08-20: "feels like u test 0 page behavior besides links" —
 * he was right: the page's own JS could fail (bad selector, CSP, throw on
 * parse) while every API assert stayed green. These asserts are on the DOM
 * the folds render INTO, after the page's own fetch+render ran.
 *
 * RUN: UI_PAGES_URL=http://localhost:3340 npx playwright test proving/flows/chorus-pages-render-1778.spec.cjs
 * In the pipeline the variant URL is injected, so the WERK's pages are graded.
 */
const { test, expect } = require('@playwright/test');
const BASE = process.env.UI_PAGES_URL || 'http://localhost:3340';

test.describe('#1778 chorus pages render real data', () => {
  test('/werk renders funnel counts from loom-metrics', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/werk`, { waitUntil: 'domcontentloaded' });
    // the funnel counters are the page'score fold — they must become digits,
    // not stay at their empty/placeholder state
    await expect(page.locator('#cnt-building')).toHaveText(/\d+/, { timeout: 15000 });
    await expect(page.locator('#cnt-proving')).toHaveText(/\d+/, { timeout: 15000 });
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('/chorus renders the nervous-system canvas nodes', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/chorus`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#node-chorus')).toBeVisible({ timeout: 15000 });
    await expect(page.locator('#node-clearing')).toBeVisible();
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('stylesheet actually applies — no naked-HTML page (the gathering.css 404 class)', async ({ page }) => {
    await page.goto(`${BASE}/chorus`, { waitUntil: 'load' });
    // a page whose stylesheet 404s renders with browser-default styling; the
    // body keeps the default serif font stack. Assert a styled font landed.
    const font = await page.evaluate(() => getComputedStyle(document.body).fontFamily);
    expect(font.toLowerCase()).not.toMatch(/^\s*(times|serif)\b/);
  });

  test('/loom renders the operations fold with real counts', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/loom`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#ops-deploys')).toHaveText(/\d+/, { timeout: 15000 });
    await expect(page.locator('#ops-commits')).toHaveText(/\d+/, { timeout: 15000 });
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('/borg-assessment renders the assessment grid', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/borg-assessment`, { waitUntil: 'domcontentloaded' });
    // the grid must gain CHILDREN — a rendered assessment, not an empty shell
    await expect(page.locator('#assessment-grid > *').first()).toBeVisible({ timeout: 15000 });
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('/flow renders board lanes with counts', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/flow`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#next-count')).toHaveText(/\d+/, { timeout: 15000 });
    await expect(page.locator('#done-count')).toHaveText(/\d+/, { timeout: 15000 });
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('/harvesting/icd renders the domains container', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/harvesting/icd`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#domains-container > *').first()).toBeVisible({ timeout: 15000 });
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('/model-data serves its page (HONEST floor: a stub by design — #3361)', async ({ page }) => {
    // The page is a declared placeholder; asserting rich folds would fabricate
    // behavior it does not have. The floor: it loads, titles itself, throws
    // nothing. When #3361 wires live data, THIS test must be raised with it.
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${BASE}/model-data`, { waitUntil: 'load' });
    await expect(page).toHaveTitle(/Model Data/i);
    expect(errors).toEqual([]);
  });

  test('board (Vikunja) serves its app shell', async ({ page }) => {
    const BOARD = process.env.UI_BOARD_URL || 'http://localhost:3456';
    await page.goto(BOARD, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#app, .app, [id=app]').first()).toBeAttached({ timeout: 15000 });
  });

  test('cost dashboard (Grafana) resolves and mounts', async ({ page }) => {
    const GRAF = process.env.UI_GRAFANA_URL || 'http://localhost:3100';
    await page.goto(`${GRAF}/d/cost-dashboard`, { waitUntil: 'domcontentloaded' });
    // Grafana redirects to login or renders the dashboard — both are a LIVE
    // mount; a 404 page or a dead server is the failure this guards.
    await expect(page.locator('.grafana-app, #reactRoot, [class*=grafana]').first())
      .toBeAttached({ timeout: 15000 });
  });
});
