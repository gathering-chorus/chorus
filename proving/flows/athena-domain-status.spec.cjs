// @test-type: e2e:ui — playwright: the domain page shows the API version running in production, never a typed status
// @domain: tests
/**
 * #5045 AC1 — Jeff's phone, 2026-11-25 15:47: "we did not analyze status yet". The Status tile
 * showed a word typed on the Domain row ("operating"), never checked against anything. Jeff,
 * 2026-11-26 11:08: a domain has no status of its own; a VERSION of its API has a state. This
 * card shows the one state that can be read today: the version running in production.
 *
 * The tile reads the model's own discovery (GET /owl/): each served class reports the fingerprint
 * of the shape its API was generated from. The truth each test holds the page to is asked of the
 * door directly, never through the page's lookup:
 *   - one served class: the tile shows that class's version, "in production";
 *   - several: it says how many classes report a version; each class's own is in the Vocabulary fold;
 *   - a served class that reports no version: "version not reported", never "in production";
 *   - no served class: "no API in production", never a version;
 *   - open cards on the domain never change it, and a typed status on the row is never shown.
 *
 * Page code this suite opens and reads: platform/api/public/athena/domain.html.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-status.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, PHONE, openDomainPage, serveLintOnce, serveOwnRoutes, serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');

const BEFORE_5045 = '053b45343dd35620691020bca43d17629c46aa54';

/** What the doors hold for a domain: the classes it declares that are served, each with the version it reports (or null). */
async function apiTruth(page, domain) {
  const get = async (p) => { const r = await page.request.get(BASE + p, { timeout: 20_000 }); expect(r.ok(), `GET ${p}: HTTP ${r.status()}`).toBe(true); return r.json(); };
  const row = ((await get('/owl/domains/domains?limit=200')).data || []).find((r) => r.name === domain);
  expect(row, `the model holds no domain named ${domain}`).toBeTruthy();
  const listed = (await get('/owl/')).primitives || [];
  return [].concat(row.definesVocabulary || []).filter(Boolean)
    .map((kind) => listed.find((p) => p.kind === kind)).filter(Boolean)
    .map((p) => ({ kind: p.kind, version: p.shapeVersion && p.shapeVersion !== 'unversioned' ? String(p.shapeVersion) : null }));
}

/** The tile as drawn: { state, value, label }, or null; and each Vocabulary row's text by class. */
const drawn = (page) => page.evaluate(() => {
  const clean = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const t = document.querySelector('#main .stats .stat[data-api]');
  const vocab = Object.fromEntries([...document.querySelectorAll('#main details.fold')].filter((f) => /^\s*Vocabulary/.test(f.querySelector('summary')?.textContent || ''))
    .flatMap((f) => [...f.querySelectorAll('tr')].filter((tr) => tr.querySelector('td')).map((tr) => [clean(tr.cells[0]), clean(tr.cells[1])])));
  const labels = [...document.querySelectorAll('#main .stats .stat .l')].map(clean);
  return { tile: t ? { state: t.getAttribute('data-api'), value: clean(t.querySelector('.v')), label: clean(t.querySelector('.l')) } : null, vocab, labels };
});

/** Draw the page again and wait for it. A route added after the page was opened is the one that answers. */
async function redraw(page) {
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#main .stats .stat[data-api]') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
}

/**
 * Answer the page's reads of discovery with the live answer, changed for one class by `change(entry)`.
 * Added AFTER the page is opened (the lib registers its own discovery route on open, and the later
 * route wins), then the page is drawn again.
 */
async function withDiscovery(page, change) {
  await page.route((url) => url.origin === new URL(BASE).origin && url.pathname === '/owl/', async (route) => {
    const live = await route.fetch();
    const body = await live.json();
    body.primitives = (body.primitives || []).map((p) => change({ ...p })).filter(Boolean);
    return route.fulfill({ response: live, json: body });
  });
}

test.describe('#5045 AC1 the tile shows the API version in production, never a typed status', () => {
  test.setTimeout(240_000);

  test('a domain serving one class shows that class\'s version, read live, "in production"', async ({ page }) => {
    const truth = await apiTruth(page, 'logs');
    expect(truth.length, 'logs no longer serves exactly one class: pick another live case').toBe(1);
    expect(truth[0].version, 'the logs API reports no version: pick another live case').toBeTruthy();
    await openDomainPage(page, { domain: 'logs' });
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'live', value: truth[0].version.slice(0, 8), label: 'API version · in production' });
    expect(d.vocab[truth[0].kind], 'the class\'s own row names the same version').toContain(`version ${truth[0].version.slice(0, 8)}`);
    expect(d.labels, 'no tile is labelled Status any more').not.toContain('Status');
  });

  test('a domain serving several classes says how many report a version; each is named in Vocabulary', async ({ page }) => {
    const truth = await apiTruth(page, 'alerts');
    expect(truth.length, 'alerts no longer serves several classes: pick another live case').toBeGreaterThan(1);
    const versioned = truth.filter((t) => t.version);
    await openDomainPage(page, { domain: 'alerts' });
    const d = await drawn(page);
    expect(d.tile.state).toBe('live');
    expect(d.tile.value).toBe(versioned.length === truth.length ? `${truth.length} classes` : `${versioned.length} of ${truth.length} classes`);
    expect(d.tile.label).toBe('API versions · in production');
    for (const t of truth) expect(d.vocab[t.kind], `${t.kind}'s row`).toContain(t.version ? `version ${t.version.slice(0, 8)}` : 'version not reported');
  });

  test('NEGATIVE: a served class that reports no version reads "version not reported", never "in production"', async ({ page }) => {
    // logs' one class, with the version taken out of the door's own answer (principles is in this state for real today)
    const kind = (await apiTruth(page, 'logs'))[0].kind;
    await openDomainPage(page, { domain: 'logs' });
    await withDiscovery(page, (p) => { if (p.kind === kind) delete p.shapeVersion; return p; });
    await redraw(page);
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'unreported', value: 'version not reported', label: 'API version' });
    expect(d.vocab[kind]).toContain('version not reported');
  });

  test('NEGATIVE: the API\'s own word "unversioned" is not shown as a version', async ({ page }) => {
    const kind = (await apiTruth(page, 'logs'))[0].kind;
    await openDomainPage(page, { domain: 'logs' });
    await withDiscovery(page, (p) => { if (p.kind === kind) p.shapeVersion = 'unversioned'; return p; });
    await redraw(page);
    expect((await drawn(page)).tile).toEqual({ state: 'unreported', value: 'version not reported', label: 'API version' });
  });

  test('NEGATIVE: a domain that serves no class reads "no API in production" and shows no version', async ({ page }) => {
    const truth = await apiTruth(page, 'wine-diary-wines');
    expect(truth, 'wine-diary-wines now serves a class: pick another live case').toEqual([]);
    await openDomainPage(page, { domain: 'wine-diary-wines' });
    expect((await drawn(page)).tile).toEqual({ state: 'none', value: 'no API in production', label: 'API version' });
  });

  test('NEGATIVE: a status typed on the row is never shown, and the open cards do not change the tile', async ({ page }) => {
    const truth = await apiTruth(page, 'logs');
    // the Domain row as served, with a typed status put back on it
    await page.route((url) => url.origin === new URL(BASE).origin && /\/owl\/domains(\/logs)?$/.test(url.pathname), async (route) => {
      const live = await route.fetch();
      const body = await live.json();
      for (const row of [].concat(body.data || [])) if (row && row.name === 'logs') row.status = 'retiring-typed-5045';
      return route.fulfill({ response: live, json: body });
    });
    // and a board with an open card on the domain
    await page.route((url) => /\/api\/chorus\/domain\/logs\/cards$/.test(url.pathname), (route) => route.fulfill({ status: 200, contentType: 'application/json',
      body: JSON.stringify({ domain: 'logs', cards: [{ id: '1', title: 'an open card planted by the test', owner: 'wren', status: 'WIP', priority: 'P1' }] }) }));
    await openDomainPage(page, { domain: 'logs' });
    const d = await drawn(page);
    expect(d.tile).toEqual({ state: 'live', value: truth[0].version.slice(0, 8), label: 'API version · in production' });
    expect(await page.evaluate(() => document.querySelector('#main').innerText), 'the typed status reached the page').not.toContain('retiring-typed-5045');
    expect(await page.evaluate(() => document.querySelector('#main').innerText), 'the planted card was not drawn: this proof needs it on the page').toContain('an open card planted by the test');
  });

  test('NEGATIVE: the page as it was before this card shows the typed word, and this check names it', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await serveLintOnce(page);
    await serveOwnRoutes(page);
    await serveAthenaFilesAsOf(page, BASE, BEFORE_5045);
    await page.goto(`${BASE}/athena/domain.html?d=principles`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#main .stats') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
    const d = await drawn(page);
    expect(d.tile, 'the old page has no version tile').toBeNull();
    expect(d.labels, 'the old page did not label a tile Status: this proof no longer shows the fault').toContain('Status');
  });
});
