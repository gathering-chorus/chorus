// @test-type: e2e:ui — playwright browser flow (principles-phone-4358), static page + live data proxy
/**
 * #4358 — Jeff, 2026-10-08 09:29: "the principles page renders with margin on
 * phone". The body kept a 1-inch side padding at every width, so on a 390px
 * phone the text had about half the screen. A phone keeps a small margin;
 * desktop keeps the inch.
 *
 * RUN
 *   npx playwright test proving/flows/principles-phone-4358.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const http = require('http');
const fs = require('fs');
const path = require('path');

const PUBLIC_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api', 'public');
const DATA_ORIGIN = process.env.CHORUS_API || 'http://localhost:3340';
const PORT = Number(process.env.PRINCIPLES_SPEC_PORT || 3494);
const BASE = `http://127.0.0.1:${PORT}`;
let server = null;

test.beforeAll(async () => {
  server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (url.startsWith('/api/') || url.startsWith('/owl/')) {
      http.get(`${DATA_ORIGIN}${req.url}`, (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      }).on('error', () => { res.writeHead(502).end('data origin unreachable'); });
      return;
    }
    const file = path.join(PUBLIC_DIR, url);
    if (!file.startsWith(PUBLIC_DIR) || !fs.existsSync(file)) { res.writeHead(404).end(); return; }
    res.writeHead(200, { 'content-type': 'text/html' });
    fs.createReadStream(file).pipe(res);
  });
  await new Promise((r) => server.listen(PORT, '127.0.0.1', r));
});

test.afterAll(async () => { if (server) await new Promise((r) => server.close(r)); });

async function sidePadding(page) {
  await page.goto(`${BASE}/loom/principles.html`);
  return page.evaluate(() => {
    const cs = getComputedStyle(document.body);
    return { left: parseFloat(cs.paddingLeft), right: parseFloat(cs.paddingRight), width: document.body.clientWidth };
  });
}

test('on a phone (390px) the text gets the screen, with a small side margin', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const p = await sidePadding(page);
  expect(p.left).toBeGreaterThanOrEqual(12);
  expect(p.left).toBeLessThanOrEqual(24);
  expect(p.right).toBeLessThanOrEqual(24);
});

test('NEGATIVE PROOF: on a desktop (1280px) the inch stays, so the check can tell the two apart', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const p = await sidePadding(page);
  expect(p.left).toBe(96);
  expect(p.right).toBe(96);
});
