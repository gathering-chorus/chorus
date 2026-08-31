// @test-type: e2e:ui — playwright browser flow (chorus-loom-4036), live surface
/**
 * #4036 — /loom rebuilt Chorus-native. The spec grades what the dig found broken:
 *
 *   - loom.html was a render artifact of team.ejs, hand-edited (#4031) so a
 *     re-render deleted the sections → the RENDER now carries the sections.
 *   - /loom/<role> served the hub back for every role → each role URL now
 *     renders that role's page (reflection, owns/working/challenging).
 *   - Team Pulse fetched /data/loom-metrics-*.json — March-frozen files from
 *     a generator that never existed here → the ONE data path is the live
 *     /api/loom-metrics, and the tiles hydrate from the live board.
 *   - Gathering chrome (navbar, footer, PDF/Share/Reflect, rrweb recorder)
 *     came along in the rip-out → gone.
 *
 * RUN
 *   npx playwright test proving/flows/chorus-loom-4036.spec.cjs
 *   FLOW_BASE=http://localhost:3345 npx playwright test proving/flows/chorus-loom-4036.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { execFileSync } = require('child_process');
const http = require('http');
const fs = require('fs');
const path = require('path');

const PUBLIC_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api', 'public');
const API_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api');
const DATA_ORIGIN = process.env.CHORUS_API || 'http://localhost:3340';
const OWN_PORT = Number(process.env.LOOM_SPEC_PORT || 3493);
const BASE = process.env.FLOW_BASE || `http://127.0.0.1:${OWN_PORT}`;
const BRINGS_OWN = !process.env.FLOW_BASE;

// The renders this spec grades are BUILT FROM SOURCE FIRST — that is the
// negative proof against the #4031 hand-edit failure: if the sections lived
// only in the artifact, this re-render would delete them and the spec REDs.
test.beforeAll(() => {
  execFileSync('node', [path.join(API_DIR, 'render-chorus-pages.cjs'), 'loom'], { stdio: 'pipe', env: { ...process.env, NODE_PATH: process.env.NODE_PATH || '/Users/jeffbridwell/CascadeProjects/chorus/platform/api/node_modules' } });
});

// chorus-api's routes, mirrored (server.ts: /loom → loom.html, /loom/:role → loom-<role>.html)
const ROUTE_ALIAS = {
  '/loom': 'chorus-pages/loom.html',
  '/loom/jeff': 'chorus-pages/loom-jeff.html',
  '/loom/wren': 'chorus-pages/loom-wren.html',
  '/loom/silas': 'chorus-pages/loom-silas.html',
  '/loom/kade': 'chorus-pages/loom-kade.html',
};
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };
let server = null;

test.beforeAll(async () => {
  if (!BRINGS_OWN) return;
  server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (url.startsWith('/api/') || url.startsWith('/owl/')) {
      http.get(`${DATA_ORIGIN}${req.url}`, (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      }).on('error', () => { res.writeHead(502).end('data origin unreachable'); });
      return;
    }
    let rel = url === '/' ? 'index.html' : url.replace(/^\/+/, '');
    if (ROUTE_ALIAS[url]) rel = ROUTE_ALIAS[url];
    const file = path.join(PUBLIC_DIR, rel);
    if (!file.startsWith(PUBLIC_DIR) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404).end('not found');
      return;
    }
    res.writeHead(200, { 'content-type': MIME[path.extname(file)] || 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(OWN_PORT, '127.0.0.1', resolve);
  });
});

test.afterAll(async () => {
  if (server) await new Promise((r) => server.close(r));
});

test.describe('#4036 the hub page is Chorus-native', () => {
  test('the Gathering chrome is gone and the Chorus crumb is there', async ({ page }) => {
    await page.goto(`${BASE}/loom`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('nav'), 'no Gathering navbar').toHaveCount(0);
    await expect(page.locator('footer'), 'no Gathering footer').toHaveCount(0);
    await expect(page.locator('.content-actions'), 'no PDF/Share/Reflect bar').toHaveCount(0);
    await expect(page.locator('.rec-toggle'), 'no rrweb recorder chrome').toHaveCount(0);
    await expect(page.locator('.crumb'), 'the Chorus crumb leads home').toContainText('Chorus');
    await expect(page.locator('h1.page-title'), 'the page is Loom').toHaveText('Loom');
  });

  test('four role tiles with real reflection excerpts, in order', async ({ page }) => {
    await page.goto(`${BASE}/loom`, { waitUntil: 'domcontentloaded' });
    const tiles = page.locator('.role-tile');
    await expect(tiles, 'four tiles').toHaveCount(4);
    for (const [key, phrase] of [
      ['jeff', 'holds the vision'],
      ['wren', 'why this, why now'],
      ['silas', 'worries about the foundation'],
      ['kade', 'makes it real'],
    ]) {
      const t = page.locator(`[data-role-tile="${key}"]`);
      await expect(t, `${key} tile exists`).toHaveCount(1);
      await expect(t.locator('.role-tile-excerpt p'), `${key} excerpt is the real reflection`).toContainText(phrase);
      await expect(t.locator(`a[href="/loom/${key}"]`).first(), `${key} links its page`).toHaveCount(1);
    }
    // Jeff is direction, not throughput: no shipped stat, no mic.
    await expect(page.locator('[data-role-tile="jeff"] .voice-mic'), 'no mic on Jeff').toHaveCount(0);
    await expect(page.locator('#shipped-num-jeff'), 'no shipped stat on Jeff').toHaveCount(0);
  });

  test('the substrate sections survived the re-render (#4031 negative proof made real)', async ({ page }) => {
    // beforeAll re-rendered loom.html from team.ejs. If the sections lived
    // only in the hand-edited artifact, they are gone now and this REDs.
    await page.goto(`${BASE}/loom`, { waitUntil: 'domcontentloaded' });
    const p = page.locator('#principles');
    await expect(p.locator('h2'), 'Principles section').toHaveText('Principles');
    for (const href of ['/loom/principles.html', '/loom/principles-list.html', '/loom/principles-reference-impl.html']) {
      await expect(p.locator(`a[href="${href}"]`), `Principles carries ${href}`).toHaveCount(1);
    }
    const d = page.locator('#decisions-policies-practices');
    await expect(d.locator('h2'), 'Decisions · Policies · Practices section').toContainText('Decisions');
    for (const href of ['/loom/decisions.html', '/loom/policies.html', '/loom/cookbook-substrate-class-domain.html']) {
      await expect(d.locator(`a[href="${href}"]`), `section carries ${href}`).toHaveCount(1);
    }
  });

  test('one data path: the live endpoint, never the March files', async ({ request }) => {
    const src = await (await request.get(`${BASE}/loom`)).text();
    expect(src.includes('/api/loom-metrics'), 'the page fetches the live endpoint').toBe(true);
    expect(/\/data\/loom-metrics/.test(src), 'no fetch of the frozen /data/loom-metrics files').toBe(false);
    expect(src.includes('/api/chorus/context/board/wip'), 'the tiles hydrate from the live board').toBe(true);
  });

  test('Team Pulse renders live numbers from /api/loom-metrics', async ({ page }) => {
    await page.goto(`${BASE}/loom`, { waitUntil: 'networkidle' });
    await expect(page.locator('#team-pulse h2'), 'Team Pulse present').toHaveText('Team Pulse');
    for (const id of ['range-buttons', 'vs-card', 'role-card', 'brief-card', 'wip-health-card', 'quality-card', 'ops-card']) {
      await expect(page.locator(`#${id}`), `${id} present`).toHaveCount(1);
    }
    // The live endpoint answered and the card headline moved off its 0 default.
    const headline = await page.locator('#vs-card h4').innerText();
    expect(headline, `vs-card headline "${headline}" carries a live count`).toMatch(/^[1-9]\d* cards$/);
    // The ops stage element exists — 57% of the board must never vanish again.
    await expect(page.locator('#vs-ops'), 'the ops stage is rendered').toHaveCount(1);
  });
});

test.describe('#4036 Analytics — the migrated #2116 instrument', () => {
  const FIXTURE = {
    range: '30d', totalCards: 7,
    flow: { Done: 4, WIP: 1, Next: 2 },
    throughput: 3,
    dailyThroughput: [{ date: '2026-08-29', count: 1 }, { date: '2026-08-30', count: 2 }],
    roleFitness: [{ role: 'wren', shipped: 2, avgLeadHours: 108, maxLeadHours: 144 }],
    bottleneck: { status: 'Next', count: 2 },
  };

  test('the section renders fitness, flow, bottleneck, and the daily series', async ({ page }) => {
    await page.route('**/api/loom-analytics*', (r) => r.fulfill({ json: FIXTURE }));
    await page.goto(`${BASE}/loom`, { waitUntil: 'networkidle' });
    for (const id of ['an-fitness-card', 'an-flow-card', 'an-daily-card']) {
      await expect(page.locator(`#${id}`), `${id} present`).toHaveCount(1);
    }
    await expect(page.locator('#an-fitness'), 'role fitness carries lead time').toContainText('lead avg 108h');
    await expect(page.locator('#an-fitness'), 'and the shipped count').toContainText('2 shipped');
    await expect(page.locator('#an-flow'), 'cumulative flow counts by stage').toContainText('Done');
    await expect(page.locator('#an-bottleneck'), 'the bottleneck is named').toContainText('Bottleneck: Next (2 open cards)');
    await expect(page.locator('#an-daily div[title="2026-08-30: 2"]'), 'daily bars carry the data').toHaveCount(1);
  });

  test('an empty range says so; a dead endpoint fails LOUD (negative proofs)', async ({ page }) => {
    await page.route('**/api/loom-analytics*', (r) => r.fulfill({
      json: { ...FIXTURE, roleFitness: [], dailyThroughput: [], bottleneck: null },
    }));
    await page.goto(`${BASE}/loom`, { waitUntil: 'networkidle' });
    await expect(page.locator('#an-fitness')).toContainText('Nothing shipped in range');
    await expect(page.locator('#an-daily')).toContainText('No cards done in range');

    await page.unroute('**/api/loom-analytics*');
    await page.route('**/api/loom-analytics*', (r) => r.fulfill({ status: 500, json: { error: 'down' } }));
    await page.goto(`${BASE}/loom`, { waitUntil: 'networkidle' });
    await expect(page.locator('#an-bottleneck'), 'a broken fetch reads as broken, never as all-quiet').toContainText('Could not load analytics');
  });
});

test.describe('#4036 the role pages are real', () => {
  test('each /loom/<role> renders that role, not the hub', async ({ page }) => {
    for (const [key, name, phrase] of [
      ['jeff', 'Jeff', 'holds the vision'],
      ['wren', 'Wren', 'point guard'],
      ['silas', 'Silas', 'sets screens'],
      ['kade', 'Kade', 'finishes at the rim'],
    ]) {
      await page.goto(`${BASE}/loom/${key}`, { waitUntil: 'domcontentloaded' });
      await expect(page.locator('h1.page-title'), `/loom/${key} is ${name}'s page`).toHaveText(name);
      await expect(page.locator('.role-reflection'), `${name}'s reflection is the migrated prose`).toContainText(phrase);
      // The defect this fixes: the hub served back at every role URL.
      await expect(page.locator('#team-pulse'), `/loom/${key} is NOT the hub`).toHaveCount(0);
      await expect(page.locator('a.back-link[href="/loom"]'), 'back-link to the hub').toHaveCount(1);
      for (const heading of ['What I Own', "What's Working", "What's Challenging"]) {
        await expect(page.getByRole('heading', { name: heading }), `${heading} section`).toHaveCount(1);
      }
    }
  });
});

test.describe('#4036 the checks can fail', () => {
  // NEGATIVE PROOF 1 — the not-the-hub check REDs when a role URL serves the
  // hub (the exact pre-fix behavior, synthesized from the real hub markup).
  test('the role-page check REDs on a hub served at a role URL', async ({ page, request }) => {
    const hub = await (await request.get(`${BASE}/loom`)).text();
    await page.setContent(hub);
    await expect(page.locator('#team-pulse'), 'the hub HAS a team-pulse — so a role URL serving it is caught').toHaveCount(1);
    await expect(page.locator('.role-reflection'), 'and no single-role reflection').toHaveCount(0);
  });

  // NEGATIVE PROOF 2 — the one-data-path check REDs on the old fetch shape.
  test('the data-path check REDs on the frozen-file fetch', () => {
    const old = "fetch('/data/loom-metrics-' + days + 'd.json')";
    expect(/\/data\/loom-metrics/.test(old), 'the forbidden shape is detected').toBe(true);
  });

  // NEGATIVE PROOF 3 — an unknown role must 404, not soft-serve something.
  test('an unknown role answers 404', async ({ request }) => {
    const res = await request.get(`${BASE}/loom/borg`);
    expect(res.status(), '/loom/borg is nobody').toBe(404);
  });
});
