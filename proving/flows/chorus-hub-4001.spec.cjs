// @test-type: e2e:ui — playwright browser flow (chorus-hub-4001), live surface
/**
 * #4001 — the /chorus front door, restructured to the canvas Jeff shaped on
 * 2026-08-23/24 and approved on 2026-08-25.
 *
 * THE SHAPE BEING ASSERTED, and why each part is a claim rather than decoration:
 *
 *   - A product tile carries ITS OWN views. Jeff, 2026-08-24: "domains and
 *     value stream are children of athena". Before this, Domains and Value
 *     Stream sat on Athena but so did Model, which is not a view of Athena at
 *     all — it is a view of the whole graph.
 *   - "The Model" is a row of whole-graph viewers, PEERS of the products, not
 *     an "Ontology Views" appendix at the bottom. Jeff, 2026-08-23: the keeper
 *     viewers are first-class tiles off /chorus.
 *   - The 47 unclaimed pages leave the front door for an Archive page, linked
 *     from the footer with its live count. They are not hidden — hiding them
 *     would remove the decision each one is waiting on — they are moved.
 *
 * WHAT THIS CANNOT BE. The tempting version of this spec asserts that some
 * text appears somewhere on the page, which stays green through every
 * restructure that keeps the words and loses the structure. So each test names
 * the SECTION and the TILE, and the last two tests prove the checks can fail:
 * one against a hub missing the row, one against a link that no longer resolves.
 *
 * RUN
 *   FLOW_BASE=http://localhost:3340 npx playwright test proving/flows/chorus-hub-4001.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const http = require('http');
const fs = require('fs');
const path = require('path');

/**
 * THIS SPEC GRADES THE HUB IN THIS TREE, not the deployed one.
 *
 * The lesson is #2725's, paid for over five pipeline runs: a check on a page's
 * STRUCTURE, pointed at the deployed copy, grades the old structure for the
 * whole life of the card that changes it. Red until the land, and the land
 * needs it green. So with no FLOW_BASE the spec serves this werk's
 * `platform/api/public` itself and proxies the two data routes the hub reads
 * (`/owl/*`, `/api/*`) to the running chorus-api, which is where products and
 * the page inventory legitimately live — the DATA is the system's, the MARKUP
 * is the branch's. FLOW_BASE still wins, so grading the deployed hub after a
 * land is one env var.
 */
const PUBLIC_DIR = path.resolve(__dirname, '..', '..', 'platform', 'api', 'public');
const DATA_ORIGIN = process.env.CHORUS_API || 'http://localhost:3340';
const OWN_PORT = Number(process.env.HUB_SPEC_PORT || 3491);
const BASE = process.env.FLOW_BASE || `http://127.0.0.1:${OWN_PORT}`;
const HUB = `${BASE}/`;
const BRINGS_OWN = !process.env.FLOW_BASE;

const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };
let server = null;

test.beforeAll(async () => {
  if (!BRINGS_OWN) return;
  server = http.createServer((req, res) => {
    const url = req.url.split('?')[0];
    if (url.startsWith('/owl/') || url.startsWith('/api/')) {
      // Data comes from the running system; markup comes from this branch.
      http.get(`${DATA_ORIGIN}${req.url}`, (up) => {
        res.writeHead(up.statusCode || 502, up.headers);
        up.pipe(res);
      }).on('error', () => { res.writeHead(502).end('data origin unreachable'); });
      return;
    }
    const rel = url === '/' ? 'index.html' : url.replace(/^\/+/, '');
    const file = path.join(PUBLIC_DIR, rel);
    // Serve only inside the public dir, and 404 LOUDLY for anything absent —
    // a soft 200 on a removed page is the failure this card's negative proof
    // exists to catch, so the harness must not manufacture one.
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

/** The Model row, found by its heading rather than by position. */
function modelSection(page) {
  return page.locator('.section').filter({ has: page.getByRole('heading', { name: /^the model$/i }) });
}

test.describe('#4001 the hub is the shape of the product', () => {
  test('The Model row holds the whole-graph viewers', async ({ page }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    const section = modelSection(page);
    await expect(section, 'the hub has a section headed "The Model"').toHaveCount(1);

    // Class Atlas is NOT here on purpose — Silas, 2026-08-25: a tile exists
    // only when its target exists, and #3992 has not shipped its route. Its
    // absence is asserted below, so the day #3992 lands this test says so.
    for (const name of ['Instance Explorer', 'Model & Data Hub']) {
      await expect(
        section.locator('.product h2', { hasText: name }),
        `"${name}" is a tile in The Model row`,
      ).toHaveCount(1);
    }
  });

  test('no tile in The Model row points somewhere it does not go', async ({ page }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    await expect(
      modelSection(page).locator('.product h2', { hasText: 'Class Atlas' }),
      'Class Atlas stays out of the row until #3992 gives it a route — a tile '
        + 'pointed at the ER diagram would be a tile lying about what it opens',
    ).toHaveCount(0);
  });

  test('the Convergence tile carries the ICD view that actually exists', async ({ page, request }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    const conv = page.locator('#products .product.convergence');
    await expect(conv, 'the Convergence product tile rendered').toHaveCount(1);
    const icd = conv.locator('.links a', { hasText: /icd flow/i });
    await expect(icd, 'Convergence carries ICD Flow (Jeff, 2026-08-24)').toHaveCount(1);
    const href = await icd.getAttribute('href');
    const res = await request.get(href.startsWith('http') ? href : `${BASE}${href}`);
    expect(res.ok(), `ICD Flow points at a live page, got ${res.status()} for ${href}`).toBeTruthy();
  });

  test('the Athena tile carries its own child views, and not the whole-graph one', async ({ page }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    const athena = page.locator('#products .product.athena');
    await expect(athena, 'the Athena product tile rendered').toHaveCount(1);

    const hrefs = await athena.locator('.links a').evaluateAll((as) => as.map((a) => a.getAttribute('href')));
    for (const child of ['domains.html', 'value-stream.html']) {
      expect(hrefs.some((h) => h && h.includes(child)), `Athena links ${child}`).toBe(true);
    }
    // Model is a view of the graph, not of Athena — it moved to The Model row.
    // Asserted as ABSENCE on the tile, which is the half a "does it appear
    // anywhere" check cannot see.
    expect(
      hrefs.some((h) => h && /\/athena\/model\.html$/.test(h)),
      'the whole-graph Model link no longer sits on the Athena tile',
    ).toBe(false);
  });

  test('the unclaimed pages moved to an Archive behind one footer link', async ({ page, request }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    const archive = page.locator('a[href$="archive.html"]');
    await expect(archive, 'the hub links the Archive exactly once').toHaveCount(1);

    // The count rides the link, so the pile stays visible as a number even
    // though its 47 links no longer sit on the front door.
    await expect(archive, 'the Archive link carries the live unclaimed count')
      .toHaveText(/Archive — unclaimed & retired pages \(\d+\)/);

    const res = await request.get(`${BASE}/chorus-pages/archive.html`);
    expect(res.ok(), 'the Archive page itself resolves').toBeTruthy();

    // And the hub no longer carries the pile inline.
    await expect(
      page.getByRole('heading', { name: /^unclaimed/i }),
      'the hub has no inline Unclaimed section any more',
    ).toHaveCount(0);
  });

  test('the Archive lists the unclaimed pages the hub counted', async ({ page }) => {
    await page.goto(HUB, { waitUntil: 'domcontentloaded' });
    const link = page.locator('a[href$="archive.html"]');
    // The count arrives with the inventory fetch, so wait for it rather than
    // reading the pre-fetch label and calling the pile empty.
    await expect(link, 'the Archive link picked up its count').toHaveText(/\(\d+\)/);
    const label = await link.innerText();
    const counted = Number((label.match(/\((\d+)\)/) || [])[1]);
    expect(Number.isFinite(counted) && counted > 0, 'the hub counted a pile').toBe(true);

    await page.goto(`${BASE}/chorus-pages/archive.html`, { waitUntil: 'domcontentloaded' });
    await expect(page.locator('#unclaimed li').first(), 'the Archive rendered rows').toBeVisible();
    const listed = await page.locator('#unclaimed li').count();
    // One inventory, two renderings — a drift here means the two surfaces are
    // reading different sources, which is the defect this pairing prevents.
    expect(listed, `Archive lists ${listed}, hub counted ${counted} — same inventory`).toBe(counted);
  });
});

test.describe('#4001 the structure checks can fail', () => {
  // NEGATIVE PROOF 1 — a hub without The Model row must RED. Synthesised, so it
  // runs without a second server: the locator strategy above is applied to
  // markup that omits the section, and must find nothing.
  test('the Model-row check REDS on a hub that lost the section', async ({ page }) => {
    await page.setContent('<div class="section"><h2>Ontology Views</h2>'
      + '<div class="product"><h2>Class Atlas</h2></div></div>');
    // The words are all present — "Class Atlas" is right there. Only the
    // SECTION is wrong, which is exactly what a text-contains check misses.
    await expect(modelSection(page), 'no section headed "The Model" — must be zero').toHaveCount(0);
    await expect(page.locator('.product h2', { hasText: 'Class Atlas' }),
      'while the tile text alone still matches, proving the text check is not the gate').toHaveCount(1);
  });

  // NEGATIVE PROOF 2 — a retired tile's old link must 404 LOUDLY. A restructure
  // that leaves a soft 200 on a removed page is the silent half of the defect:
  // the link looks alive, serves something, and nobody learns it moved.
  test('a path this restructure does not serve answers 404, not a soft 200', async ({ request }) => {
    const res = await request.get(`${BASE}/athena/ontology-views-retired-4001.html`);
    expect(res.status(), 'an unserved hub path must answer 404').toBe(404);
  });
});

test.describe('#4001 one query, one truth', () => {
  // Silas, 2026-08-25: "does the archive call the SAME endpoint+params as the
  // hub, or re-derive? If re-derived, the drift returns on the next page."
  // Sharing a SOURCE was not enough — both pages read /api/chorus/ui-pages and
  // still disagreed 77 vs 47, because only one of them sent ?products=. So the
  // CALL is what is shared, and this test grades that rather than the numbers
  // agreeing on one lucky day: neither page may build the question itself.
  test('neither surface re-derives the inventory question', async ({ request }) => {
    const files = ['/index.html', '/chorus-pages/archive.html'];
    for (const f of files) {
      const res = await request.get(`${BASE}${f}`);
      expect(res.ok(), `${f} serves`).toBeTruthy();
      const src = await res.text();
      expect(src.includes('loadPageInventory'), `${f} asks through the shared call`).toBe(true);
      expect(
        /ui-pages['"`]\s*\)\s*\+|products=/.test(src),
        `${f} must NOT assemble the ui-pages query itself — that is how 77-vs-47 happened`,
      ).toBe(false);
    }
  });

  // NEGATIVE PROOF — the check above must RED on a page that re-derives. The
  // string it forbids is exactly the code that caused the drift.
  test('the shared-call check REDS on a page that builds the query itself', () => {
    const reDerived = "const res = await fetch(bp('/api/chorus/ui-pages') + '?products=' + labels.join(','));";
    expect(/ui-pages['"`]\s*\)\s*\+|products=/.test(reDerived), 'the forbidden shape is detected').toBe(true);
    expect(reDerived.includes('loadPageInventory'), 'and it does not use the shared call').toBe(false);
  });
});
