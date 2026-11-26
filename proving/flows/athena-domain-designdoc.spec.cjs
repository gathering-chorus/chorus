// @test-type: e2e:ui — playwright: the domain page's Design doc fold shows only a document that belongs to the domain
// @domain: tests
/**
 * #5045 AC4 — Jeff's phone, 2026-11-25 15:47: "design doc is stale and not in graph". The
 * principles page offered Loom's design document as its own. Measured 11-26: identity pointed at
 * security's document, logs and security "linked" to the very page the reader was on, and two
 * domains named a document the model does not hold.
 *
 * The rule the page is held to: a document is shown only when the Document row ITSELF names this
 * domain (its hasDomain) and it leads somewhere other than this page. When none does, the fold is
 * still drawn and says so; it never offers another domain's document.
 *
 * Page code this suite opens and reads: platform/api/public/athena/domain.html.
 * The truth is asked of the model door directly (GET /owl/knowledge/documents?hasDomain=), never
 * through the page's own lookup.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-designdoc.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, PHONE, openDomainPage, serveLintOnce, serveOwnRoutes, serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');

// main as it stood when this card was pulled: the page that offered Loom's document to principles
const BEFORE_5045 = '053b45343dd35620691020bca43d17629c46aa54';

/** The Design doc fold as drawn: { state, text, links: [{ text, path }] }, or null when the page drew none. */
const foldAsDrawn = (page) => page.evaluate(() => {
  const f = [...document.querySelectorAll('#main > details.fold')].find((d) => /^\s*Design doc/.test(d.querySelector('summary')?.textContent || ''));
  if (!f) return null;
  const body = f.querySelector('.body');
  return { state: f.getAttribute('data-dd'), text: body.innerText.replace(/\s+/g, ' ').trim(),
    links: [...body.querySelectorAll('a[href]')].map((a) => ({ text: a.textContent.trim(), path: new URL(a.href).pathname + new URL(a.href).search })) };
});

/** The documents the model door says belong to a domain: [{ name, docTitle, docHref }]. */
async function ownDocuments(page, domain) {
  const r = await page.request.get(`${BASE}/owl/knowledge/documents?hasDomain=${encodeURIComponent(domain)}&limit=200`, { timeout: 20_000 });
  expect(r.ok(), `the door did not answer for ${domain}'s documents: HTTP ${r.status()}`).toBe(true);
  return ((await r.json()).data || []).filter((d) => [].concat(d.hasDomain || []).some((v) => String(v).split('#').pop() === domain));
}

/**
 * Answer the page's reads of the Domain rows with the live answer, changed in one place: the
 * named domain's hasDesignDoc is `value` (null: the row names none). Everything else is as served.
 */
async function withDesignDoc(page, domain, value) {
  const change = (row) => { if (row && row.name === domain) { if (value === null) delete row.hasDesignDoc; else row.hasDesignDoc = value; } };
  await page.route((url) => url.origin === new URL(BASE).origin && /\/owl\/domains(\/[^/]+)?$/.test(url.pathname) && !url.pathname.endsWith('/policies'), async (route) => {
    const live = await route.fetch();
    if (!live.ok()) return route.fulfill({ response: live });
    const body = await live.json();
    [].concat(body.data || []).forEach(change);
    return route.fulfill({ response: live, json: body });
  });
}

test.describe('#5045 AC4 the Design doc fold shows only a document that belongs to the domain', () => {
  test.setTimeout(240_000);

  test('a domain whose document names it is offered that document, by its title', async ({ page }) => {
    const own = (await ownDocuments(page, 'alerts')).find((d) => d.name === 'alerts-service-design');
    expect(own, 'the model no longer holds alerts-service-design for alerts: pick another live case').toBeTruthy();
    await withDesignDoc(page, 'alerts', 'document-alerts-service-design');
    await openDomainPage(page, { domain: 'alerts' });
    const fold = await foldAsDrawn(page);
    expect(fold, 'the page drew no Design doc fold').not.toBeNull();
    expect(fold.state).toBe('own');
    expect(fold.links).toEqual([{ text: own.docTitle, path: own.docHref }]);
  });

  test("NEGATIVE: another domain's document is not offered; the fold says none belongs and names what it left out", async ({ page }) => {
    const spine = (await ownDocuments(page, 'spine')).find((d) => d.name === 'spine-product-design');
    expect(spine, 'the model no longer holds spine-product-design for spine: pick another live case').toBeTruthy();
    expect((await ownDocuments(page, 'alerts')).map((d) => d.name), 'spine-product-design must not belong to alerts for this proof').not.toContain('spine-product-design');
    await withDesignDoc(page, 'alerts', 'document-spine-product-design');
    await openDomainPage(page, { domain: 'alerts' });
    const fold = await foldAsDrawn(page);
    expect(fold.state).toBe('none');
    expect(fold.links, "another domain's document was offered as a link").toEqual([]);
    expect(fold.text).toContain('No design document in the model belongs to alerts');
    expect(fold.text).toContain('spine-product-design');
  });

  test('NEGATIVE: a file name and a document the model does not hold are not offered either', async ({ page }) => {
    await withDesignDoc(page, 'alerts', ['quality-service-design.html', 'document-no-such-document-5045']);
    await openDomainPage(page, { domain: 'alerts' });
    const fold = await foldAsDrawn(page);
    expect(fold.state).toBe('none');
    expect(fold.links).toEqual([]);
    expect(fold.text).toContain('quality-service-design.html');
    expect(fold.text).toContain('no-such-document-5045');
  });

  test('NEGATIVE: a document whose address is this very page is not offered as a link to follow', async ({ page }) => {
    const own = (await ownDocuments(page, 'logs')).find((d) => d.name === 'domain-design-logs');
    expect(own && own.docHref, 'the model no longer holds domain-design-logs pointing at the logs page: pick another live case').toBe('/athena/domain.html?d=logs');
    await withDesignDoc(page, 'logs', 'document-domain-design-logs');
    await openDomainPage(page, { domain: 'logs' });
    const fold = await foldAsDrawn(page);
    expect(fold.links, 'the fold linked the page to itself').toEqual([]);
    expect(fold.state).toBe('none');
    expect(fold.text).toContain('This page is generated from the model');
  });

  test('a domain that names no document still draws the fold, and it says so', async ({ page }) => {
    // principles as its row will read once this card's rows land: it names no design document
    await withDesignDoc(page, 'principles', null);
    await openDomainPage(page, { domain: 'principles' });
    const fold = await foldAsDrawn(page);
    expect(fold, 'a domain with no design document drew no fold: the reader cannot tell "none" from "not loaded"').not.toBeNull();
    expect(fold.state).toBe('none');
    expect(fold.links).toEqual([]);
    expect(fold.text).toContain('No design document in the model belongs to principles');
    expect(fold.text).toContain('This page is generated from the model');
  });

  test("NEGATIVE: the page as it was before this card offers another domain's document, and the check names it", async ({ page }) => {
    // identity's row planted to name security's document, the fault measured on 11-26
    expect((await ownDocuments(page, 'identity')).map((d) => d.name)).not.toContain('service-design-security');
    await page.setViewportSize(PHONE);
    await serveLintOnce(page);
    await serveOwnRoutes(page);
    await withDesignDoc(page, 'identity', 'document-service-design-security');
    await serveAthenaFilesAsOf(page, BASE, BEFORE_5045);
    await page.goto(`${BASE}/athena/domain.html?d=identity`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#main .stats') && document.querySelector('#code-slot details'), null, { timeout: 45_000 });
    const fold = await foldAsDrawn(page);
    expect(fold, 'the old page drew no Design doc fold for identity').not.toBeNull();
    expect(fold.links.map((l) => l.text), "the old page did not offer security's document: this proof no longer shows the fault").toEqual(['Service Design — Security']);
  });
});
