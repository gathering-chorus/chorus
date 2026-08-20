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
});
