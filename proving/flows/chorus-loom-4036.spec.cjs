// @test-type: e2e:ui — playwright browser flow (chorus-loom-4036), live surface
// @domain: tests — placed by chorus-crawl's domain rules (#4538)
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
test.afterEach(require('./lib/athena-page-5034.cjs').letRoutesGo);   // nightly 11-28: no route outlives its test
const { execFileSync } = require('child_process');
const http = require('http');
const fs = require('fs');
const path = require('path');

const PUBLIC_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api', 'public');
const API_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api');
const DATA_ORIGIN = process.env.CHORUS_API || 'http://localhost:3340';
// A free port by default: a fixed 3493 collided with demo envs that bind it (#4530 run 21).
const OWN_PORT = Number(process.env.LOOM_SPEC_PORT || 0);
let BASE = process.env.FLOW_BASE || '';
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
  BASE = `http://127.0.0.1:${server.address().port}`;
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
    for (const href of ['/loom/principles.html', '/loom/principles-list.html']) {
      await expect(p.locator(`a[href="${href}"]`), `Principles carries ${href}`).toHaveCount(1);
    }
    const d = page.locator('#decisions-policies-practices');
    await expect(d.locator('h2'), 'Decisions · Policies · Practices section').toContainText('Decisions');
    for (const href of ['/loom/decisions.html', '/loom/policies.html']) {
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
    // #4028 — the page's own rendering is under test, not the last 90 minutes
    // of prod's spine: /api/loom-metrics tallies a 4 MB tail of chorus.log, so
    // this red whenever no card was pulled or closed recently (twice tonight,
    // 19:05 and 19:15). Bring the data, as the analytics test below does.
    await page.route('**/api/loom-metrics*', (r) => r.fulfill({
      json: { board: { done: 3, total: 7 }, weekly_throughput: { '2026-W35': 3 }, reject_stats: { rate: 0, deploys: 2 }, operations: { deploys: 2 } },
    }));
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

// =============================================================================================
// #5044 AC4 — the three pages the principles domain page links to show their data on a phone,
// through the sign-in door.
//
// Jeff's phone at the public address, 2026-11-25 15:50: "all the ui page links have issues". The
// UI Pages fold of /athena/domain.html?d=principles links /loom/principles.html,
// /loom/principles-list.html and /loom/policy-counts.html. #5041's test followed each link and
// took a 200 for an answer. Each page did open, and then asked for its data at a root-fixed
// address, which under /chorus leaves the door: the pages drew "Failed to load". The counts page
// also loads the shared Boston clock from /vendor/chorus-time, which was not on the door's list.
//
// Opened at 390 px through the REAL door (lib startDoor: the real guard, this tree's allow list,
// the live chorus-api behind it), at both mounts, with this tree's copy of the three pages:
//   - principles and principles-list draw one entry for every principle the model holds;
//   - policy-counts draws its policies with the time they were measured, in Boston time;
//   - nothing a page asks for leaves the door or is refused by it; no page scrolls sideways.
// The door: the clock's one directory is served to a signed-in reader, and a made-up neighbour
// under /vendor is not (Silas, 18:28: "Add only that one directory line").
// NEGATIVE: the three pages as they are on main, under /chorus, each draw no rows and say so.
//
// Page code opened and read: platform/api/public/loom/principles.html,
// platform/api/public/loom/principles-list.html, platform/api/public/loom/policy-counts.html;
// the list: config/share-allowlist.txt.
// =============================================================================================
const { startDoor, signedInPage, BASE: LIVE_API, ROOT: TREE } = require('./lib/athena-page-5034.cjs');

const LOOM_PAGES = ['principles.html', 'principles-list.html', 'policy-counts.html'];
const MAIN_BEFORE_5044 = '9461b1d8d';

/** A signed-in phone page whose three Loom pages come from this tree, or from a commit (`sha`). */
async function loomPhone(browser, door, sha = null) {
  const { page, context, refused } = await signedInPage(browser, door, { lint: 'live' });
  await page.route((url) => url.origin === door.origin && LOOM_PAGES.some((f) => url.pathname === `${door.mount}/loom/${f}`), (route) => {
    const f = new URL(route.request().url()).pathname.split('/').pop();
    const body = sha ? execFileSync('git', ['-C', TREE, 'show', `${sha}:platform/api/public/loom/${f}`], { encoding: 'utf8' })
      : fs.readFileSync(path.join(PUBLIC_DIR, 'loom', f), 'utf8');
    return route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body });
  });
  return { page, context, refused };
}

/** Open one of the three and wait until it has drawn rows or said it could not. Returns what it drew. */
async function drawnLoomPage(page, door, file) {
  await page.goto(`${door.origin}${door.mount}/loom/${file}`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => {
    const t = document.body.innerText;
    return /Failed to load|Could not load/.test(t) || document.querySelector('details.principle, #list li, #rows .row');
  }, null, { timeout: 120_000 });
  return page.evaluate(() => ({
    rows: document.querySelectorAll('details.principle, #list li, #rows .row').length,
    says: (document.body.innerText.match(/(Failed to load|Could not load)[^\n]*/) || [null])[0],
    sub: document.getElementById('sub')?.textContent.trim() || null,
    over: document.documentElement.scrollWidth - window.innerWidth,
    textWidth: Math.round(document.querySelector('details.principle, #list li, #rows .row')?.getBoundingClientRect().width || 0),
  }));
}

for (const [where, mount] of [['under /chorus (the public apex)', '/chorus'], ['at the root (the door\'s own host)', '']]) {
  test.describe(`#5044 the principles domain's three UI pages through the sign-in door, ${where}`, () => {
    test.setTimeout(300_000);
    let door;
    test.beforeAll(async () => { door = await startDoor({ mount }); });
    test.afterAll(() => door && door.stop());

    test('principles and principles-list each draw every principle the model holds; nothing leaves the door', async ({ browser, request }) => {
      const held = ((await (await request.get(`${LIVE_API}/owl/v1/principles/principles`, { timeout: 30_000 })).json()).data || []).length;
      expect(held, 'the model holds principles: this check needs some').toBeGreaterThan(0);
      const { page, context, refused } = await loomPhone(browser, door);
      for (const file of ['principles.html', 'principles-list.html']) {
        const drawn = await drawnLoomPage(page, door, file);
        expect.soft(drawn.says, `${file} says it could not load`).toBeNull();
        expect.soft(drawn.rows, `${file}: the model holds ${held} principles`).toBe(held);
        expect.soft(drawn.over, `${file} is ${drawn.over}px wider than a phone`).toBeLessThanOrEqual(1);
        // a phone-width column of text, not a desktop page shrunk to fit or squeezed by an inch of margin each side
        expect.soft(drawn.textWidth, `${file}: its text column is ${drawn.textWidth}px of 390`).toBeGreaterThan(300);
      }
      expect([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      await context.close();
    });

    test('policy-counts draws its policies and when they were measured, in Boston time; nothing leaves the door', async ({ browser }) => {
      const { page, context, refused } = await loomPhone(browser, door);
      const drawn = await drawnLoomPage(page, door, 'policy-counts.html');
      expect.soft(drawn.says, 'the page says it could not load').toBeNull();
      expect.soft(drawn.rows, 'the page lists the gate\'s policies').toBeGreaterThan(0);
      expect.soft(drawn.sub, 'the line above them').toMatch(/^\d+ arch policies, worst first · \d+ narrowed · measured \d{4}-\d\d-\d\d \d\d:\d\d Boston$/);
      expect.soft(drawn.over, `the page is ${drawn.over}px wider than a phone`).toBeLessThanOrEqual(1);
      expect([...new Set(refused)], `requests that left the door or that it refused:\n${[...new Set(refused)].join('\n')}`).toEqual([]);
      await context.close();
    });

    test('the door serves the Boston clock\'s one directory to a signed-in reader, and no neighbour under /vendor', async ({ browser }) => {
      const { context } = await signedInPage(browser, door, { lint: 'live' });
      const clock = await context.request.get(`${door.origin}${door.mount}/vendor/chorus-time/boston.js`, { maxRedirects: 0 });
      expect(clock.status(), 'the clock script through the door').toBe(200);
      expect(await clock.text(), 'and it is the clock, not a sign-in page').toMatch(/bostonMinute/);
      // NEGATIVE: the list names ONE directory. A neighbour, the /vendor root, and a path that climbs out of it are refused.
      for (const p of ['/vendor/no-such-thing-5044/x.js', '/vendor/', '/vendor/chorus-time/../../config/share-allowlist.txt']) {
        const r = await context.request.get(`${door.origin}${door.mount}${p}`, { maxRedirects: 0 });
        expect(r.status(), `${p} must not be served through the door`).not.toBe(200);
      }
      await context.close();
    });
  });
}

test.describe('#5044 NEGATIVE: the three pages as they are on main draw nothing under /chorus', () => {
  test.setTimeout(300_000);
  test('each page on main, opened under /chorus, draws no rows and says it could not load', async ({ browser }) => {
    const door = await startDoor({ mount: '/chorus' });
    try {
      const { page, context, refused } = await loomPhone(browser, door, MAIN_BEFORE_5044);
      for (const file of LOOM_PAGES) {
        const drawn = await drawnLoomPage(page, door, file);
        expect.soft(drawn.rows, `${file} on main draws no rows under /chorus`).toBe(0);
        expect.soft(drawn.says, `${file} on main says it could not load`).toMatch(/Failed to load|Could not load/);
      }
      expect(refused.join('\n'), 'and its reads leave the door').toMatch(/LEFT THE DOOR GET \/(owl\/v1\/principles\/principles|api\/chorus\/policies\/counts|vendor\/chorus-time\/boston\.js)/);
      await context.close();
    } finally { door.stop(); }
  });
});
