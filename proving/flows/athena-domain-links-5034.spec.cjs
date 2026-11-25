// @test-type: e2e:ui — playwright: every link on the Athena domain page and its model view leads somewhere
// @domain: tests
/**
 * #5034 AC2 — Jeff's phone, 2026-11-24: on the principles pages the vocabulary link answered
 * 404, the design doc link answered 404, and the model view said "Principle — Not served"
 * while the door held 14 principles. A reader who taps a link must land on something.
 *
 * Every same-origin link on both pages is followed:
 *   - the ops view   /athena/domain.html?d=<domain>
 *   - the model view /domains/<domain>, including one card per instance
 * The model view is one template that answers 200 for any name, so those links are opened
 * and read, never trusted on status (lib/athena-page-5034.cjs deadLinkReason).
 *
 * Page code this suite opens and reads, named so a change to any of it runs this suite:
 *   platform/api/public/athena/domain.html, platform/api/public/athena/domains-view.html,
 *   platform/api/public/athena/model-fold.js (the instance cards and their links).
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-links-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, DOMAIN, openDomainPage, openModelView, sameOriginLinks, deadLinks } = require('./lib/athena-page-5034.cjs');

test.describe(`#5034 every link on the ${DOMAIN} domain pages leads somewhere`, () => {
  test.setTimeout(240_000);

  test('the ops view has no dead link', async ({ page }) => {
    await openDomainPage(page);
    const links = await sameOriginLinks(page);
    expect(links.length, 'the page drew no links at all').toBeGreaterThan(5);
    const dead = await deadLinks(page, links);
    expect(dead, `dead links on the ops view:\n${dead.join('\n')}`).toEqual([]);
  });

  // Whether the model view says the right thing about each class (served or not, a card per
  // row) is the true-data suite's question (athena-domain-true-5034). Here: every link drawn.
  test('the model view has no dead link, instance cards included', async ({ page }) => {
    await openModelView(page);
    const links = await sameOriginLinks(page);
    expect(links.length, 'the model view drew no links at all').toBeGreaterThan(5);
    const dead = await deadLinks(page, links);
    expect(dead, `dead links on the model view:\n${dead.join('\n')}`).toEqual([]);
  });

  test('NEGATIVE: a dead link and a link to a name the model does not hold are both named', async ({ page }) => {
    await openDomainPage(page);
    const planted = [`${BASE}/owl/no-such-collection-5034`, `${BASE}/domains/${DOMAIN}/no-such-instance-5034`, `${BASE}/athena/no-such-page-5034.html`];
    const dead = await deadLinks(page, planted);
    expect(dead.length, `all three planted links must be reported, got:\n${dead.join('\n')}`).toBe(3);
    // and a live one is not: the check can tell the two apart
    expect(await deadLinks(page, [`${BASE}/domains/${DOMAIN}`, `${BASE}/athena/domains.html`])).toEqual([]);
  });
});

// =============================================================================================
// #5041 AC1 — one link per piece of data.
//
// Jeff walked the principles page on his phone, 2026-11-25 10:22 to 10:34: "schema->owl->instances
// seems same as vocabulary link", "we dont want 2-3 links to same data", and on Conformance "i
// want to be able to click into and see policy instances". The page had three routes to the same
// 14 principles (the tile, the Vocabulary link, the embedded Model fold) and 54 policy names in
// plain text.
//
// Read from the drawn page, through the real sign-in door, at both mounts, at phone width:
//   - no two links in the page's content lead to the same place;
//   - exactly ONE link goes into the model view, and it is labelled "Shape and OWL";
//   - each served class has exactly one link to its rows page, labelled with the store's own count;
//   - every policy the Conformance fold names is a link, and each one, followed, answers 200 and
//     opens a page that draws that policy (the model view is one template: it is opened and read).
// NEGATIVE: the page as Jeff walked it (cut from git at #5040) is red, naming the repeated route
// and the unlinked policies; a link to a policy the model does not hold is named dead.
//
// Named limit: "the page's content" is everything under #main. The ladder and the breadcrumb
// above it are the family's navigation, drawn by every page; they repeat each other (value
// stream) and the Product tile, and are not held to this rule here.
//
// Page code read as drawn: platform/api/public/athena/domain.html (tiles, Vocabulary,
// Conformance), platform/api/public/athena/domains-view.html (a policy's page).
// =============================================================================================
const { doorTruth, startDoor, signedInPage, openOldDomainPage } = require('./lib/athena-page-5034.cjs');

const MOUNTS_5041 = [['under /chorus (the public apex)', '/chorus'], ['at the root (the door\'s own host)', '']];
const OPS = `/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`;

/** Run in the page: every link in the page's content, and every policy the Conformance fold names. */
function drawnLinks() {
  const text = (n) => n.textContent.replace(/\s+/g, ' ').trim();
  return {
    links: [...document.querySelectorAll('#main a[href]')].map((a) => ({ href: a.href, text: text(a) })),
    policies: [...document.querySelectorAll('#conf-slot tr[data-policy]')].map((tr) => ({ name: tr.dataset.policy, href: tr.querySelector('a[href]')?.href || null })),
    // the UI Pages fold: each row's route (its first cell) and the link that cell holds, if any
    pages: [...document.querySelectorAll('details.fold')].filter((f) => /^UI Pages/.test(text(f.querySelector('summary'))))
      .flatMap((f) => [...f.querySelectorAll('tr')].filter((tr) => tr.cells[0]?.tagName === 'TD').map((tr) => ({ route: text(tr.cells[0]), href: tr.cells[0].querySelector('a[href]')?.href || null }))),
  };
}

/** Every way the drawn links break "one link per piece of data", as plain lines. */
function linkFaults({ links, policies, pages = [] }, { under, truth }) {
  const out = [];
  const at = (h) => h.replace(under, '');
  const by = new Map();
  for (const l of links) by.set(l.href, [...(by.get(l.href) || []), l.text]);
  for (const [href, texts] of by) if (texts.length > 1) out.push(`repeated route: ${at(href)} is linked ${texts.length} times (${texts.map((t) => `"${t.slice(0, 40)}"`).join(', ')})`);
  // the model view and every page under it (a class, an instance) is one piece of data: the model's own rows
  const model = links.filter((l) => new URL(l.href).pathname === new URL(under).pathname.replace(/\/$/, '') + `/domains/${DOMAIN}` || l.href.startsWith(`${under}/domains/${DOMAIN}/`));
  if (model.length !== 1) out.push(`model view: ${model.length} links lead into it, there must be exactly one (${model.slice(0, 4).map((l) => at(l.href)).join(', ')}${model.length > 4 ? ', …' : ''})`);
  else if (model[0].text !== 'Shape and OWL') out.push(`model view: its one link reads "${model[0].text}", not "Shape and OWL"`);
  for (const t of truth.filter((c) => c.collection)) {
    const rows = links.filter((l) => l.href === `${under}/athena/rows.html?kind=${encodeURIComponent(t.kind)}`);
    const want = `${t.held} ${t.held === 1 ? 'row' : 'rows'} of ${t.kind}`;
    if (rows.length !== 1) out.push(`${t.kind}: ${rows.length} links to its rows page, there must be exactly one`);
    else if (rows[0].text !== want) out.push(`${t.kind}: its rows link reads "${rows[0].text}", the store holds ${t.held} (it must read "${want}")`);
  }
  if (!policies.length) out.push('Conformance names no policy: nothing to link');
  const bare = policies.filter((p) => p.href !== `${under}/domains/policies/${encodeURIComponent(p.name)}`);
  if (bare.length) out.push(`policies not linked to their own page: ${bare.length} of ${policies.length} (${bare.slice(0, 3).map((p) => p.name).join(', ')}${bare.length > 3 ? ', …' : ''})`);
  // Jeff, 10:59: "the ui interfaces fold must give clickable links". A route is the page's own address.
  const plainPages = pages.filter((p) => p.href !== under + p.route);
  if (plainPages.length) out.push(`UI Pages: ${plainPages.length} of ${pages.length} routes are not links to their page (${plainPages.slice(0, 3).map((p) => p.route).join(', ')}${plainPages.length > 3 ? ', …' : ''})`);
  return out;
}

/**
 * Why a policy link is dead, or null. The door must answer 200 for it (chorus-api answers 404 for a
 * policy the model does not hold), and the page it opens must draw that policy, not its
 * "Not found" note. Both are asked: the template alone answers 200 for any name.
 */
async function policyLinkDead(context, tab, href, name) {
  const r = await context.request.get(href, { timeout: 30_000, maxRedirects: 0 }).catch((e) => ({ status: () => 0, why: String(e.message).split('\n')[0] }));
  if (r.status() !== 200) return `${name}: the door answers ${r.status() || r.why}`;
  await tab.goto(href, { waitUntil: 'domcontentloaded' });
  await tab.waitForFunction(() => { const app = document.getElementById('app'); return app && app.textContent.trim() && !/Loading the model/.test(app.textContent); }, null, { timeout: 30_000 });
  const drawn = await tab.evaluate(() => ({ note: document.querySelector('#app > .note')?.textContent.replace(/\s+/g, ' ').trim().slice(0, 90) || null, text: document.getElementById('app').innerText }));
  if (drawn.note) return `${name}: the page says: ${drawn.note}`;
  return drawn.text.includes(name) ? null : `${name}: the page it opens does not name the policy`;
}

/** Open the ops view through the door and wait for the Conformance fold's verdict (the lint runs about 12 s). */
async function openOpsThroughDoor(page, door) {
  await page.goto(door.origin + door.mount + OPS, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelector('#code-slot details') && document.querySelector('#svc-slot details') && document.querySelector('#conf-slot details')
    && !/loading the materialized view|loading live from the model|running the domain lint/.test(document.body.innerText), null, { timeout: 180_000 });
}

for (const [where, mount] of MOUNTS_5041) {
  test.describe(`#5041 one link per piece of data on the ${DOMAIN} ops view, through the sign-in door ${where}`, () => {
    test.setTimeout(420_000);
    // The ops view is opened ONCE for the three tests. Each open runs the domain lint on the server
    // (12 s alone), and the door gives its upstream 30 s: three opens in a row, beside the other
    // suites, pushed one lint past that and the fold read "Cannot reach" (first run, 11-25 11:20).
    let door, page, context, drawn;
    test.beforeAll(async ({ browser }) => {
      test.setTimeout(420_000);
      door = await startDoor({ mount });
      ({ page, context } = await signedInPage(browser, door));
      await openOpsThroughDoor(page, door);
      drawn = await page.evaluate(drawnLinks);
    });
    test.afterAll(async () => { if (context) await context.close(); if (door) door.stop(); });

    test('no two links share a destination; one link to the model view, one per class to its rows, one per policy', async () => {
      const truth = await doorTruth(page);
      expect(truth.some((t) => t.collection && t.held > 0), `the model serves no class with rows for ${DOMAIN}: nothing to check`).toBe(true);
      const faults = linkFaults(drawn, { under: door.origin + door.mount, truth });
      expect(faults, `the ops view breaks "one link per piece of data":\n${faults.join('\n')}`).toEqual([]);
    });

    test('every policy link, followed, answers 200 and draws that policy', async () => {
      const { policies } = drawn;
      expect(policies.length, 'the Conformance fold named no policy').toBeGreaterThan(0);
      const dead = [];
      // what the ops view drew is already read (`drawn`), so its own page follows the links:
      // that page serves this tree's model-view template (signedInPage)
      for (const p of policies) {
        if (!p.href) { dead.push(`${p.name}: not a link`); continue; }
        const why = await policyLinkDead(context, page, p.href, p.name);
        if (why) dead.push(why);
      }
      expect(dead, `policy links that lead nowhere (of ${policies.length}):\n${dead.join('\n')}`).toEqual([]);
    });

    test('every UI Pages route is a link, and each one, followed, answers 200 through the door', async () => {
      expect(drawn.pages.length, `the UI Pages fold listed no page for ${DOMAIN}: nothing to check`).toBeGreaterThan(0);
      const dead = [];
      for (const p of drawn.pages) {
        if (p.href !== door.origin + door.mount + p.route) { dead.push(`${p.route}: ${p.href ? 'links to ' + p.href.replace(door.origin, '') : 'not a link'}`); continue; }
        const r = await context.request.get(p.href, { timeout: 30_000, maxRedirects: 0 }).catch((e) => ({ status: () => 0, why: String(e.message).split('\n')[0] }));
        if (r.status() !== 200) dead.push(`${p.route}: the door answers ${r.status() || r.why}`);
      }
      // a made-up page beside them is refused, so a 200 above is the page's own answer
      const none = await context.request.get(`${door.origin}${door.mount}/loom/no-such-page-5041.html`, { maxRedirects: 0 });
      expect(dead, `UI Pages routes that are not links or lead nowhere (of ${drawn.pages.length}):\n${dead.join('\n')}`).toEqual([]);
      expect(none.status(), 'a page that does not exist must not answer 200 through the door').not.toBe(200);
    });
  });
}

test.describe('#5041 NEGATIVE: the link checks are red on the page Jeff walked, and on a policy the model does not hold', () => {
  test.setTimeout(420_000);
  let door;
  test.beforeAll(async () => { door = await startDoor({ mount: '/chorus' }); });
  test.afterAll(() => door && door.stop());

  test('the ops view as it was at #5040 is named: the repeated route, the routes into the model view, the unlinked policies', async ({ browser }) => {
    const { page, context } = await signedInPage(browser, door);
    const truth = await doorTruth(page);
    // The policies answer is written here, two real policy names: this proof is about how the OLD
    // page drew a policy name, not about the lint. Run live it took the lint past the 30 s the door
    // gives its upstream when the four suites ran together, and the fold read "Cannot reach"
    // (11-25 11:50). The positive tests above read the live answer through the door.
    await page.route((url) => url.pathname === `${door.mount}/api/chorus/policies/counts`, (route) => route.fulfill({ status: 200, contentType: 'application/json',
      body: JSON.stringify({ measuredAt: '2026-11-25T16:00:00.000Z', policies: ['arch-domain-directory-layout', 'domain-folds-filled'].map((name) => ({ name, label: name, verdict: 'pass', count: 0, baseline: 0, scope: 'global' })) }) }));
    await openOldDomainPage(page, door.origin + door.mount);
    await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
    const drawn = await page.evaluate(drawnLinks);
    const faults = linkFaults(drawn, { under: door.origin + door.mount, truth }).join('\n');
    await context.close();
    expect(faults, 'the tile and the Model fold both linked the model view').toMatch(new RegExp(`^repeated route: /domains/${DOMAIN} is linked 2 times`, 'm'));
    expect(faults, 'the Model fold linked every instance as well').toMatch(/^model view: \d\d+ links lead into it/m);
    expect(faults, 'the Vocabulary link carried no count').toMatch(/its rows link reads "rows of \w+"/);
    expect(faults, 'every policy name was plain text').toMatch(/^policies not linked to their own page: 2 of 2 \(arch-domain-directory-layout, domain-folds-filled\)$/m);
    expect(drawn.pages.length, 'the old page listed its UI pages').toBeGreaterThan(0);
    expect(faults, 'every UI Pages route was plain text').toMatch(new RegExp(`^UI Pages: ${drawn.pages.length} of ${drawn.pages.length} routes are not links`, 'm'));
  });

  test('a link to a policy the model does not hold is named dead, and a real one is not', async ({ browser }) => {
    const { page, context } = await signedInPage(browser, door);
    const under = door.origin + door.mount;
    expect(await policyLinkDead(context, page, `${under}/domains/policies/no-such-policy-5041`, 'no-such-policy-5041')).toMatch(/^no-such-policy-5041: the door answers 404$/);
    // the door's 200 alone is not trusted: the template answers 200 for any name it is handed, and its note is read
    await page.goto(`${under}/domains/policies/no-such-policy-5041`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelector('#app > .note'), null, { timeout: 30_000 });
    expect(await page.evaluate(() => document.querySelector('#app > .note').textContent)).toMatch(/not found|no such/i);
    expect(await policyLinkDead(context, page, `${under}/domains/policies/arch-domain-directory-layout`, 'arch-domain-directory-layout')).toBeNull();
    // and the fault finder itself: a policy with no link, and one linked to another policy's page, are both counted
    const faults = linkFaults({ links: [], policies: [{ name: 'a', href: null }, { name: 'b', href: `${under}/domains/policies/a` }, { name: 'c', href: `${under}/domains/policies/c` }] }, { under, truth: [] });
    expect(faults.join('\n')).toMatch(/policies not linked to their own page: 2 of 3 \(a, b\)/);
    await context.close();
  });
});
