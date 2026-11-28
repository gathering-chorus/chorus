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
test.afterEach(require('./lib/athena-page-5034.cjs').letRoutesGo);   // nightly 11-28: no route outlives its test
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
    && !/loading the materialized view|loading live from the model|running the domain lint|reading the last check/.test(document.body.innerText), null, { timeout: 180_000 });
}

// =============================================================================================
// #5044 AC3 — a card number opens the card.
//
// Jeff's phone, 2026-11-25 15:50, on the Cards fold: "cards list seems longer but cant link to
// cards like clearing v2 does". The numbers were plain text.
//
// Read from the drawn page, through the sign-in door, at both mounts: every number in the Cards
// fold reads "#<n>" and is a link, and the link is the card's address in Clearing v2 from where
// the reader stands. Clearing v2 is its own service, so that address is not under this door:
//   the public site under /chorus      the same host, /clearing-v2/<n>
//   the public site's chorus. host     the apex host, /clearing-v2/<n> (measured 11-25: the
//                                      chorus. host answers 404 for it)
//   the home network (a port is named) the same machine, Clearing v2's own port, /<n>
// The test door has a port, so the drawn link is the home-network form; it is FOLLOWED, and the
// board it opens must show that card. The two public forms are asked of the page's own function
// (athena-flow.js cardHref) with a location handed in, on made-up host names.
// One link per card on the page: the Releases fold names the same cards and shows "#<n>" unlinked
// (the "no two links share a destination" test above holds that).
// NEGATIVE: the page before this card is red (plain numbers); a link to another card's address,
// and a card address that opens a board showing no such card, are each named.
// Named limit: the public addresses are not opened from here (they are behind the real sign-in).
// Page code read as drawn: platform/api/public/athena/facets.js, platform/api/public/athena/athena-flow.js.
// =============================================================================================
const CLEARING_V2_PORT = 3472;

/** Run in the page: the Cards fold's id cells, and the Releases fold's card cells, as drawn. */
function drawnCards() {
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const fold = (name) => [...document.querySelectorAll('details.fold')].find((f) => new RegExp(`^${name}\\b`).test(text(f.querySelector('summary'))));
  const col = (f, header) => {
    const table = f?.querySelector('table'); if (!table) return [];
    const i = [...table.querySelectorAll('th')].findIndex((th) => text(th).toLowerCase() === header.toLowerCase());
    return i < 0 ? [] : [...table.querySelectorAll('tr')].map((tr) => tr.cells[i]).filter((c) => c && c.tagName === 'TD');
  };
  const cell = (c) => ({ text: text(c), href: c.querySelector('a[href]')?.href || null });
  return { cards: col(fold('Cards'), 'id').map(cell), releases: col(fold('Releases'), 'cardId').map(cell) };
}

/** Every card number that is not "#<n>" linked to that card's own address; `addressOf(n)` is the address wanted. */
function cardFaults({ cards, releases }, addressOf) {
  const out = [];
  if (!cards.length) out.push('the Cards fold lists no card: nothing to link');
  for (const c of cards) {
    const n = (/^#(\d+)$/.exec(c.text) || [])[1];
    if (!n) out.push(`Cards: "${c.text}" is not a card number written "#<n>"`);
    else if (!c.href) out.push(`Cards: ${c.text} is plain text, not a link`);
    else if (c.href !== addressOf(n)) out.push(`Cards: ${c.text} links to ${c.href}, the card is at ${addressOf(n)}`);
  }
  for (const r of releases) if (!/^#\d+$/.test(r.text)) out.push(`Releases: "${r.text}" is not written "#<n>"`);
  return out;
}

/** Why a card's link does not show the card, or null: the board it opens must name that card. */
async function cardLinkDead(tab, href, n) {
  try {
    const r = await tab.goto(href, { waitUntil: 'domcontentloaded', timeout: 30_000 });
    if (!r || r.status() !== 200) return `#${n}: ${href} answers ${r ? r.status() : 'nothing'}`;
    if (new URL(tab.url()).searchParams.get('card') !== String(n)) return `#${n}: ${href} opens ${tab.url()}, which asks for no such card`;
    // the board's card drawer writes the card's title, or "Card #<n> not found on board" (read 11-25).
    // The number alone proves nothing: the drawer prints it for a card that does not exist too.
    // Wait for its ANSWER, not its first text: the title holds a placeholder while the board looks the
    // card up. The answer is "not found" in the title, or the card's own status line (#<n> …) under it.
    await tab.waitForFunction((id) => /not found/i.test(document.getElementById('drawer-card-title')?.textContent || '')
      || new RegExp(`#${id}\\b`).test(document.getElementById('drawer-card-cws')?.textContent || ''), String(n), { timeout: 30_000 });
    const title = (await tab.evaluate(() => document.getElementById('drawer-card-title').textContent)).replace(/\s+/g, ' ').trim();
    return /not found/i.test(title) ? `#${n}: the board says "${title}"` : null;
  } catch (e) { return `#${n}: the board it opens does not show the card (${String(e.message || e).split('\n')[0]})`; }
}

for (const [where, mount] of MOUNTS_5041) {
  test.describe(`#5041 one link per piece of data on the ${DOMAIN} ops view, through the sign-in door ${where}`, () => {
    test.setTimeout(420_000);
    // The ops view is opened ONCE for the three tests. Each open runs the domain lint on the server
    // (12 s alone), and the door gives its upstream 30 s: three opens in a row, beside the other
    // suites, pushed one lint past that and the fold read "Cannot reach" (first run, 11-25 11:20).
    let door, page, context, drawn, drawnAtStart;
    test.beforeAll(async ({ browser }) => {
      test.setTimeout(420_000);
      door = await startDoor({ mount });
      ({ page, context } = await signedInPage(browser, door));
      // The policies read is the live answer, asked of chorus-api once per test process (lib
      // serveLintOnce, which signedInPage applies). This suite is about the LINKS the page draws
      // from that answer. That the door serves the read is held by the folds suite's "the ops
      // view: every fold is filled…" test, which reads it through the door.
      await openOpsThroughDoor(page, door);
      drawn = await page.evaluate(drawnLinks);
      drawnAtStart = await page.evaluate(drawnCards);   // #5044: read now; a later test takes this page elsewhere
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

    test('#5044 every card number in the Cards fold is a link to that card, and one followed shows the card', async ({ browser }) => {
      const cards = drawnAtStart;
      // the door here stands on 127.0.0.1 with a port: the home-network form of the address
      const addressOf = (n) => `http://127.0.0.1:${CLEARING_V2_PORT}/${n}`;
      const faults = cardFaults(drawnAtStart, addressOf);
      expect(faults, `card numbers that do not open their card:\n${faults.join('\n')}`).toEqual([]);
      expect(cards.releases.length, 'the Releases fold lists its cards').toBeGreaterThan(0);
      const tab = await browser.newPage();
      const n = drawnAtStart.cards[0].text.slice(1);
      const dead = await cardLinkDead(tab, drawnAtStart.cards[0].href, n);
      // NEGATIVE, on the live board: the same address for a card that does not exist is named dead
      const none = await cardLinkDead(tab, addressOf('99999999'), '99999999');
      await tab.close();
      expect(dead, 'the first card link, followed').toBeNull();
      expect(none, 'a card the board does not hold').toMatch(/^#99999999: the board says "Card #99999999 not found/);
    });
  });
}

test.describe('#5044 a card\'s address from each place a reader stands, and NEGATIVE: plain numbers and wrong addresses are named', () => {
  const { openDomainPage: openOps, serveAthenaFilesAsOf } = require('./lib/athena-page-5034.cjs');
  // main as #5041 landed it: the page before this card
  const BEFORE_5044 = '9461b1d8d';

  test('the page\'s own address for a card: under /chorus, on the chorus. host, and on the home network', async ({ page }) => {
    test.setTimeout(240_000);
    await openOps(page);
    const at = (loc, id = '5033') => page.evaluate(([l, i]) => window.cardHref(i, l), [loc, id]);
    expect(await at({ protocol: 'https:', hostname: 'garden.example', port: '' }), 'the public site: the same host').toBe('https://garden.example/clearing-v2/5033');
    expect(await at({ protocol: 'https:', hostname: 'chorus.garden.example', port: '' }), 'the chorus. host has no Clearing v2: the apex host').toBe('https://garden.example/clearing-v2/5033');
    expect(await at({ protocol: 'http:', hostname: 'library.local', port: '3340' }), 'the home network: Clearing v2\'s own port').toBe(`http://library.local:${CLEARING_V2_PORT}/5033`);
    expect(await at({ protocol: 'https:', hostname: 'garden.example', port: '' }, '#5033'), 'a number written with its #').toBe('https://garden.example/clearing-v2/5033');
    // NEGATIVE: what is not a card number gets no address (it is then drawn as plain text, never a dead link)
    for (const bad of ['', 'abc', '50x3', '../board', null]) expect(await at({ protocol: 'https:', hostname: 'garden.example', port: '' }, bad), `"${bad}" is not a card number`).toBeNull();
  });

  test('NEGATIVE: the page before this card is red on every card number, and a link to another card is named', async ({ page }) => {
    test.setTimeout(240_000);
    await page.setViewportSize({ width: 390, height: 844 });
    await serveAthenaFilesAsOf(page, BASE, BEFORE_5044);
    await page.goto(`${BASE}/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => [...document.querySelectorAll('#facets details.fold summary')].some((s) => /^Cards \(\d/.test(s.textContent)), null, { timeout: 120_000 });
    const addressOf = (n) => `http://localhost:${CLEARING_V2_PORT}/${n}`;
    const old = cardFaults(await page.evaluate(drawnCards), addressOf);
    expect(old.filter((f) => /^Cards: #\d+ is plain text, not a link$/.test(f)).length, `the old page drew its card numbers as plain text:\n${old.join('\n')}`).toBeGreaterThan(0);
    expect(old.join('\n'), 'and its Releases numbers without the #').toMatch(/^Releases: "\d+" is not written "#<n>"$/m);
    // by hand: a right link is not named; a link to another card, and a cell that is not a number, are
    expect(cardFaults({ cards: [{ text: '#5033', href: addressOf(5033) }], releases: [{ text: '#5033', href: null }] }, addressOf)).toEqual([]);
    expect(cardFaults({ cards: [{ text: '#5033', href: addressOf(5021) }, { text: '5021', href: addressOf(5021) }], releases: [] }, addressOf)).toEqual([
      `Cards: #5033 links to ${addressOf(5021)}, the card is at ${addressOf(5033)}`, 'Cards: "5021" is not a card number written "#<n>"']);
    expect(cardFaults({ cards: [], releases: [] }, addressOf)).toEqual(['the Cards fold lists no card: nothing to link']);
  });
});

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

// ---------------------------------------------------------------------------------------------
// #5042 (nightly 2026-11-28: 13 reds) — the policies lint is read once per test process and cached.
// It was read with a test's own `page.context().request`, so the test ending rejected it and every
// later test was handed that rejection. A stand-in server answers the lint slowly: the first test
// ends while it is read, and the second must still be given the answer. NEGATIVE (run by hand,
// recorded on the card): with the read as it was, the second test is red.
{
  const http = require('http');
  const { serveLintOnce } = require('./lib/athena-page-5034.cjs');
  const slow = http.createServer((req, res) => {
    if (!req.url.startsWith('/api/chorus/policies/counts')) { res.writeHead(200, { 'content-type': 'text/html' }); return res.end('<!doctype html><title>stand-in</title>'); }
    setTimeout(() => { res.writeHead(200, { 'content-type': 'application/json' }); res.end('{"red":0}'); }, 1500);
  });
  let at;
  const ask = (page) => page.evaluate((u) => fetch(u).then((r) => `${r.status}`).catch((e) => `failed: ${e.message}`), `${at}/api/chorus/policies/counts?domain=principles`);
  test.describe('#5042 the lint read outlives the test that asked it first', () => {
    test.describe.configure({ mode: 'serial' });
    test.beforeAll(async () => { await new Promise((r) => slow.listen(0, '127.0.0.1', r)); at = `http://127.0.0.1:${slow.address().port}`; });
    test.afterAll(() => new Promise((r) => slow.close(r)));
    test('a test that ends while the lint is still being read', async ({ page }) => {
      await serveLintOnce(page, at);
      await page.goto(at + '/');
      page.evaluate((u) => { fetch(u).catch(() => {}); }, `${at}/api/chorus/policies/counts?domain=principles`).catch(() => {});
      await page.waitForTimeout(200);   // the read is in flight; the test ends here
    });
    test('the next test is still given the lint, not the first test\'s ending', async ({ page }) => {
      await serveLintOnce(page, at);
      await page.goto(at + '/');
      expect(await ask(page)).toBe('200');
    });
  });
}
