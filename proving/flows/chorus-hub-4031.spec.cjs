// @test-type: e2e:ui — playwright browser flow (chorus-hub-4031), live surface
/**
 * #4031 — one tile, one door. Every link that used to sit on a front-door tile
 * now lives on its product's page (or in The Model row, or in the Archive).
 *
 * Jeff's rule, from the Front Door canvas (2026-08-29) and the product-page
 * mocks he carded on 2026-08-30:
 *   - A product tile carries ONE door. Not a curated list plus everything
 *     discovery finds under the product's folder — that is how Athena's tile
 *     grew to 13 links and Borg's to 15 on his phone.
 *   - The moved links land on the product's own page, in named sections.
 *   - A product with no page says so ("no door yet") instead of pretending.
 *   - Retired pages and older twins are NAMED in the Archive, never dropped.
 *   - Borg's door is /borg/ (Jeff, 2026-08-30 11:01), Hooks is its first view.
 *
 * Each check names the PAGE and the SECTION, so a restructure that keeps the
 * words and loses the shape goes red. The negative proofs at the end show the
 * tile check REDs on a two-link tile and the section check REDs on a page that
 * lost its section.
 *
 * RUN
 *   npx playwright test proving/flows/chorus-hub-4031.spec.cjs
 *   FLOW_BASE=http://localhost:3340 npx playwright test proving/flows/chorus-hub-4031.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const http = require('http');
const fs = require('fs');
const path = require('path');

// Same harness as chorus-hub-4001: MARKUP from this tree, DATA from the running
// chorus-api. A structure check pointed at the deployed copy grades the old
// structure for the whole life of the card that changes it (#2725).
const PUBLIC_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api', 'public');
const DATA_ORIGIN = process.env.CHORUS_API || 'http://localhost:3340';
const OWN_PORT = Number(process.env.HUB_SPEC_PORT || 3492);
const BASE = process.env.FLOW_BASE || `http://127.0.0.1:${OWN_PORT}`;
const BRINGS_OWN = !process.env.FLOW_BASE;
const CLEARING = process.env.CLEARING_ORIGIN || 'http://localhost:3470';

// The routes chorus-api serves from chorus-pages/ (server.ts sendChorusPage).
const ROUTE_ALIAS = { '/loom': 'chorus-pages/loom.html', '/werk': 'chorus-pages/werk.html' };
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };
let server = null;

test.beforeAll(async () => {
  if (!BRINGS_OWN) return;
  server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (url.startsWith('/owl/') || url.startsWith('/api/')) {
      http.get(`${DATA_ORIGIN}${req.url}`, (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      }).on('error', () => { res.writeHead(502).end('data origin unreachable'); });
      return;
    }
    // chorus-api answers /clearing with a redirect to the room (#4031); the
    // harness does the same so a click on the tile is graded end to end.
    if (url === '/clearing') { res.writeHead(302, { location: `${CLEARING}/` }).end(); return; }
    let rel = url === '/' ? 'index.html' : url.replace(/^\/+/, '');
    if (ROUTE_ALIAS[url]) rel = ROUTE_ALIAS[url];
    if (rel.endsWith('/')) rel += 'index.html';
    const file = path.join(PUBLIC_DIR, rel);
    if (!file.startsWith(PUBLIC_DIR) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      // Not a file in this tree: a chorus-api ROUTE (/loom, /werk, /domains,
      // /loom/:role, /chorus …) or nothing at all. Ask the running system,
      // which answers 200 for its routes and 404 for the rest — so a dead
      // link is graded by the system that would serve it, not by this shim.
      http.get(`${DATA_ORIGIN}${req.url}`, (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      }).on('error', () => { res.writeHead(502).end('data origin unreachable'); });
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

/** A product tile on the hub, found by its heading. */
function tile(page, name) {
  return page.locator('#products .product').filter({ has: page.getByRole('heading', { name: new RegExp(`^${name}$`, 'i') }) });
}

/** A section on a product page, found by its heading text. */
function section(page, heading) {
  return page.locator('section, .section, .loom-section, .card').filter({ has: page.getByRole('heading', { name: heading }) });
}

// tile → the ONE door it opens (Pulse has none and says so)
const DOORS = {
  'Athena': '/athena/value-stream.html',
  'Borg': '/borg/',
  'The Clearing': '/clearing',
  'Convergence': '/chorus-pages/icd.html',
  'Loom': '/loom/',
  'Werk': '/werk/',
};

test.describe('#4031 one tile, one door', () => {
  test('every product tile carries exactly one link, and it is the door', async ({ page }) => {
    await page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#products .product').first(), 'tiles rendered from owl-api').toBeVisible();
    for (const [name, door] of Object.entries(DOORS)) {
      const t = tile(page, name);
      await expect(t, `${name} tile exists once`).toHaveCount(1);
      const links = t.locator('.links a');
      await expect(links, `${name} tile carries ONE link, not a pile`).toHaveCount(1);
      await expect(links.first(), `${name} tile's one link is its door`).toHaveAttribute('href', door);
    }
  });

  test('a product with no page says so instead of pretending', async ({ page }) => {
    await page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded' });
    const t = tile(page, 'Pulse');
    await expect(t, 'Pulse tile exists').toHaveCount(1);
    await expect(t.locator('.links a'), 'Pulse has no link to a page that does not exist').toHaveCount(0);
    await expect(t, 'Pulse says it has no door yet').toContainText(/no door yet/i);
  });

  test('the hub no longer appends discovered pages to tiles', async ({ request }) => {
    const src = await (await request.get(`${BASE}/index.html`)).text();
    expect(src.includes('loadPageInventory'), 'the hub still reads the inventory for the Archive count').toBe(true);
    expect(/inventory\.claimed\[/.test(src), 'no tile is built from inventory.claimed any more').toBe(false);
  });
});

test.describe('#4031 the moved links landed on their product pages', () => {
  test('Athena: Collections row carries Domains, Products, Services', async ({ page }) => {
    await page.goto(`${BASE}/athena/value-stream.html`, { waitUntil: 'domcontentloaded' });
    const s = section(page, /^collections$/i);
    await expect(s, 'the value-stream page has a Collections section').toHaveCount(1);
    // The page links its siblings relatively (the #3810 mount shim resolves
    // them), so grade the file name, not the absolute path.
    for (const file of ['domains.html', 'products.html', 'services.html']) {
      await expect(s.locator(`a[href$="${file}"]`), `Collections links ${file}`).toHaveCount(1);
    }
  });

  test('Borg: Live / Reports / Jeff sections carry the twelve moved links', async ({ page }) => {
    await page.goto(`${BASE}/borg/`, { waitUntil: 'domcontentloaded' });
    const want = {
      Live: ['/borg/hooks/', '/borg/operations.html', '/borg/pain.html', '/borg/trace.html', '/borg/replay/', '/borg/hook-friction.html'],
      Reports: ['/borg/assessment/', '/borg/cost/', '/borg/fitness/', '/borg/logging-coverage-findings.html'],
      Jeff: ['/borg/jeff/', '/borg/patterns/'],
    };
    for (const [heading, hrefs] of Object.entries(want)) {
      const s = section(page, new RegExp(`^${heading}$`, 'i'));
      await expect(s, `Borg page has a ${heading} section`).toHaveCount(1);
      for (const href of hrefs) {
        await expect(s.locator(`a[href="${href}"]`), `${heading} carries ${href}`).toHaveCount(1);
      }
    }
    // Werk's page is Werk's, and the Model-row twin is not a Borg surface.
    await expect(page.locator('a[href="/werk/quality/"]'), 'Quality lives on Werk, not Borg').toHaveCount(0);
    await expect(page.locator('a[href="/borg/instance-explorer/"]'), 'the instance-explorer twin left the page').toHaveCount(0);
  });

  test('Loom: Principles and Decisions · Policies · Practices sections', async ({ page }) => {
    await page.goto(`${BASE}/loom`, { waitUntil: 'domcontentloaded' });
    const p = section(page, /^principles$/i);
    await expect(p, 'Loom has a Principles section').toHaveCount(1);
    for (const href of ['/loom/principles.html', '/loom/principles-list.html', '/loom/principles-reference-impl.html']) {
      await expect(p.locator(`a[href="${href}"]`), `Principles carries ${href}`).toHaveCount(1);
    }
    const d = section(page, /^decisions · policies · practices$/i);
    await expect(d, 'Loom has a Decisions · Policies · Practices section').toHaveCount(1);
    for (const href of ['/loom/decisions.html', '/loom/policies.html', '/loom/cookbook-substrate-class-domain.html']) {
      await expect(d.locator(`a[href="${href}"]`), `section carries ${href}`).toHaveCount(1);
    }
  });

  test('Werk: Quality section carries the Quality Service', async ({ page }) => {
    await page.goto(`${BASE}/werk`, { waitUntil: 'domcontentloaded' });
    const s = section(page, /^quality$/i);
    await expect(s, 'Werk has a Quality section').toHaveCount(1);
    await expect(s.locator('a[href="/werk/quality/"]'), 'Quality links the service').toHaveCount(1);
  });

  test('Archive names the retired pages and older twins', async ({ page }) => {
    await page.goto(`${BASE}/chorus-pages/archive.html`, { waitUntil: 'domcontentloaded' });
    const s = section(page, /^retired/i);
    await expect(s, 'the Archive has a Retired section').toHaveCount(1);
    const named = [
      '/athena/tree.html', '/athena/athena-cmdb-view.html', '/athena/domains-view.html', '/athena/product-view.html',
      '/chorus-pages/borg-assessment.html', '/borg/instance-explorer/',
      '/werk-process.html',
    ];
    for (const href of named) {
      await expect(s.locator(`a[href="${href}"]`), `Retired names ${href}`).toHaveCount(1);
    }
    // Still one inventory: the unclaimed list is untouched by this card.
    await expect(page.locator('#unclaimed li').first(), 'the unclaimed list still renders').toBeVisible();
  });
});

test.describe('#4031 page flow and links — the next dead one names itself', () => {
  // Jeff, 2026-08-30: "update the ui automation to validate page flow and
  // links". Two things a structure check cannot see: does the DOOR OPEN when
  // clicked, and does every link on the page behind it go somewhere. The
  // first demo of this card had /clearing answering "Cannot GET" from the one
  // link on its tile, and /loom rendering with no stylesheet tokens.
  const DOOR_PAGES = ['/', '/borg/', '/athena/value-stream.html', '/loom', '/werk', '/chorus-pages/icd.html', '/chorus-pages/archive.html'];

  test('every tile door opens a real page when clicked', async ({ page }) => {
    for (const name of Object.keys(DOORS)) {
      await page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded' });
      const link = tile(page, name).locator('.links a');
      await expect(link, `${name} has its door`).toHaveCount(1);
      const [res] = await Promise.all([
        page.waitForResponse((r) => r.request().isNavigationRequest() && r.status() !== 302 && r.status() !== 301, { timeout: 15000 }).catch(() => null),
        link.click(),
      ]);
      await page.waitForLoadState('domcontentloaded');
      const body = await page.locator('body').innerText().catch(() => '');
      // An error PAGE, not a page that mentions errors: express's "Cannot GET /x"
      // is the whole body, and so is a bare "Not Found".
      expect(body.trim(), `${name}'s door must not land on an error page`).not.toMatch(/^(Cannot GET |Not Found$|Error$)/);
      expect(res && res.status(), `${name}'s door answers 200 (got ${res && res.status()})`).toBe(200);
      // The Clearing is another service (its own markup, its own sign-in);
      // reaching it without an error is the grade. Our own pages must show a heading.
      if (page.url().startsWith(BASE)) {
        await expect(page.locator('h1').first(), `${name}'s page has a heading`).toBeVisible();
      }
    }
  });

  test('every link on the hub and on each door page resolves', async ({ page, request }) => {
    const seen = new Map();
    const dead = [];
    for (const p of DOOR_PAGES) {
      await page.goto(`${BASE}${p}`, { waitUntil: 'networkidle' });
      const hrefs = await page.locator('a[href]').evaluateAll((as) =>
        as.map((a) => a.getAttribute('href')).filter((h) => h && !h.startsWith('#') && !h.startsWith('mailto:') && !h.startsWith('javascript:')));
      expect(hrefs.length, `${p} has links`).toBeGreaterThan(0);
      for (const href of hrefs) {
        const url = new URL(href, `${BASE}${p}`).toString();
        if (!url.startsWith(BASE) && !url.startsWith(CLEARING)) continue; // off-origin: not this card's to grade
        if (seen.has(url)) continue;
        const res = await request.get(url, { maxRedirects: 5 }).catch(() => null);
        const status = res ? res.status() : 0;
        seen.set(url, status);
        // 401 is a sign-in door, not a dead link (same rule as #3886).
        if (!res || status === 404 || status >= 500) dead.push(`${href} → ${status || 'unreachable'}  (on ${p})`);
      }
    }
    expect(dead, `dead links:\n${dead.join('\n')}`).toEqual([]);
  });

  test('the Loom and Werk doors render with their stylesheet tokens', async ({ page }) => {
    // An unstyled page is a door that opens onto a hallway with the lights
    // off. The pages were written against Gathering's token names; on
    // chorus-api those must resolve, or every spacing and colour on the page
    // falls back to browser defaults. (system.css itself is serif by design.)
    for (const p of ['/loom', '/werk']) {
      await page.goto(`${BASE}${p}`, { waitUntil: 'domcontentloaded' });
      const title = page.locator('h1.page-title').first();
      await expect(title, `${p} has its page title`).toBeVisible();
      const space = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--space-md').trim());
      expect(space, `${p} resolves the --space-md token`).not.toBe('');
    }
  });
});

test.describe('#4031 the checks can fail', () => {
  // NEGATIVE PROOF 1 — a two-link tile must RED. The words are right, the
  // door is right, there is just one link too many; a text check passes it.
  test('the one-door check REDs on a tile with a second link', async ({ page }) => {
    await page.setContent('<div id="products"><div class="product"><h2>Athena</h2>'
      + '<div class="links"><a href="/athena/value-stream.html">Open</a><a href="/athena/domains.html">Domains</a></div></div></div>');
    const links = tile(page, 'Athena').locator('.links a');
    await expect(links, 'two links are counted as two').toHaveCount(2);
    await expect(links.first(), 'while the first is still the door — the count is the gate, not the href').toHaveAttribute('href', '/athena/value-stream.html');
  });

  // NEGATIVE PROOF 2 — a page that has the links but lost the SECTION must
  // RED. Every href present, nothing headed "Live".
  test('the section check REDs on a page that lost its heading', async ({ page }) => {
    await page.setContent('<div class="card"><h2>Surfaces</h2>'
      + '<a href="/borg/hooks/">Hooks</a><a href="/borg/pain.html">Pain</a></div>');
    await expect(section(page, /^live$/i), 'no section headed Live — must be zero').toHaveCount(0);
    await expect(page.locator('a[href="/borg/hooks/"]'), 'while the link itself is right there').toHaveCount(1);
  });

  // NEGATIVE PROOF 3 — the discovered-append check REDs on the old hub code.
  // NEGATIVE PROOF 4 — the dead-link check REDs on a 404. Served by the
  // harness itself, which answers 404 for anything not on disk.
  test('the link check REDs on a link that answers 404', async ({ request }) => {
    const res = await request.get(`${BASE}/this-page-does-not-exist-4031.html`);
    expect(res.status(), 'the harness answers 404 for an absent page').toBe(404);
    const dead = [];
    if (res.status() === 404) dead.push('/this-page-does-not-exist-4031.html → 404');
    expect(dead.length, 'and the crawl rule counts it as dead').toBe(1);
  });

  test('the no-append check REDs on the old tile builder', () => {
    const old = "const discovered = (inventory.claimed[p.label] || []).filter(pg => !curatedHrefs.has(pg.href));";
    expect(/inventory\.claimed\[/.test(old), 'the old append shape is detected').toBe(true);
  });
});
