// @test-type: e2e:ui — playwright: the four parts #5045 made true are drawn at phone width through the real sign-in door
// @domain: tests
/**
 * #5045 AC5 — Jeff reads these pages on his phone, at the public address, behind the sign-in
 * door (an allow list, a session, GET and HEAD only, a /chorus mount). A page that is right at
 * localhost and broken there is broken (2026-11-25 08:06: "the page is way too broken to do an
 * interview", every suite green).
 *
 * Four domains, each a different case, opened through the REAL door (lib startDoor) at phone width:
 *   principles         the domain Jeff is finishing; its API reports no version yet (#5046)
 *   logs               one served class
 *   alerts             several served classes
 *   wine-diary-wines   serves no class
 * On each: the version tile, the Gaps fold, the Dependencies fold and the Design doc fold are
 * drawn and say something; nothing the page asked for was refused by the door or left it; no
 * part runs off the side of the phone.
 * The Gaps read is new with this card: the door must let it through (it sits under an address the
 * allow list already holds), asked of the door directly. NEGATIVE: an address the list does not
 * hold is refused by the same door, and a fold pushed wider than the phone is named.
 *
 * Page code this suite opens and reads: platform/api/public/athena/domain.html,
 * platform/api/public/athena/facets.js (the Dependencies fold).
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-four-folds-door.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { PHONE, startDoor, signedInPage, serveOwnGaps } = require('./lib/athena-page-5034.cjs');

const DOMAINS = ['principles', 'logs', 'alerts', 'wine-diary-wines'];
const GAP = '5048';   // logs' walked gap: logs out of /tmp

/** Run in the page: the four parts as drawn, and where each sits. */
function fourParts() {
  const clean = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const vw = document.documentElement.clientWidth;
  const fold = (re) => [...document.querySelectorAll('#main details.fold')].find((f) => re.test(f.querySelector('summary')?.textContent || ''));
  const box = (el) => { if (!el) return null; const b = el.getBoundingClientRect(); return { left: Math.round(b.left), right: Math.round(b.right), width: Math.round(b.width) }; };
  const part = (el) => (el ? { text: clean(el.querySelector('.body') || el), box: box(el) } : null);
  const tile = document.querySelector('#main .stats .stat[data-api]');
  return { vw, bodyOver: document.documentElement.scrollWidth - window.innerWidth,
    tile: tile ? { state: tile.getAttribute('data-api'), text: clean(tile), box: box(tile) } : null,
    gaps: part(fold(/^\s*Gaps/)), deps: part(fold(/^\s*Dependencies/)), designDoc: part(fold(/^\s*Design doc/)) };
}

/** Every part that is missing, empty, unread, or off the side of the phone. */
function partFaults(d) {
  const out = [];
  if (d.bodyOver > 0) out.push(`the page scrolls sideways by ${d.bodyOver}px`);
  for (const [name, p] of [['the version tile', d.tile], ['Gaps', d.gaps], ['Dependencies', d.deps], ['Design doc', d.designDoc]]) {
    if (!p) { out.push(`${name}: not drawn`); continue; }
    if (!p.text) out.push(`${name}: drawn empty`);
    if (/could not read|loading/i.test(p.text)) out.push(`${name}: ${p.text.slice(0, 70)}`);
    if (p.box.left < 0 || p.box.right > d.vw) out.push(`${name}: runs off the phone (left ${p.box.left}, right ${p.box.right}, screen ${d.vw})`);
  }
  return out;
}

test.describe('#5045 AC5 the four parts read on a phone, through the sign-in door under /chorus', () => {
  let door;
  test.beforeAll(async () => { test.setTimeout(120_000); door = await startDoor({ mount: '/chorus' }); });
  test.afterAll(() => { if (door) door.stop(); });

  for (const domain of DOMAINS) {
    test(`${domain}: the tile, Gaps, Dependencies and Design doc are drawn, nothing is refused, nothing runs off the side`, async ({ browser }) => {
      test.setTimeout(300_000);
      const { page, context, refused } = await signedInPage(browser, door, PHONE);
      try {
        await serveOwnGaps(page, { logs: GAP });
        await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=${encodeURIComponent(domain)}`, { waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => document.querySelector('#code-slot details') && document.querySelector('#gaps-slot details') && document.querySelector('#facets-deps details')
          && !/loading the materialized view/.test(document.getElementById('facets').textContent), null, { timeout: 180_000 });
        const d = await page.evaluate(fourParts);
        expect(partFaults(d), `${domain} at ${PHONE.width}px through the door:\n${partFaults(d).join('\n')}`).toEqual([]);
        expect(refused, `the door refused, or the page left the door:\n${refused.join('\n')}`).toEqual([]);
        // which case the domain is, from the door's own discovery (asked directly): no served class,
        // a served class that reports no version (principles, until #5046), or a version in production
        const get = async (p) => (await context.request.get(`${door.origin}${door.mount}${p}`, { timeout: 30_000 })).json();
        const disc = await get('/owl/');
        const row = ((await get('/owl/domains/domains?limit=200')).data || []).find((r) => r.name === domain);
        const served = [].concat(row.definesVocabulary || []).map((k) => (disc.primitives || []).find((p) => p.kind === k)).filter(Boolean);
        const versioned = served.filter((p) => p.shapeVersion && p.shapeVersion !== 'unversioned');
        expect(d.tile.state, `the tile says which case ${domain} is (${served.length} served, ${versioned.length} with a version)`).toBe(!served.length ? 'none' : versioned.length ? 'live' : 'unreported');
        if (domain === 'logs') expect(d.gaps.text, 'the stated gap is drawn as its card').toContain(`#${GAP}`);
      } finally { await context.close(); }
    });
  }

  test('the door lets the new Gaps read through, and refuses an address its list does not hold', async ({ browser }) => {
    test.setTimeout(120_000);
    const { context } = await signedInPage(browser, door, PHONE);
    try {
      // asked of the door itself, not through a page (a page's read of it is answered from this tree before the land).
      // The door's own refusal of an unlisted address is a 404 too, so the status cannot tell the
      // two apart. What can: the door's refusal is its own text, and an answer it let through
      // comes from the server behind it. Before the land that server has no such route (its own
      // 404); after it, the route answers 200 with the domain's gaps.
      const fromServer = async (r) => r.status() === 200 ? typeof (await r.json()).domain === 'string' : /^upstream 404\b/.test((await r.text()).trim());   // the door's own words for a 404 the server gave
      const refusedByDoor = async (r) => r.status() === 404 && /^path not shared\b/.test((await r.text()).trim());   // the door's own words for an address off its list
      const gaps = await context.request.get(`${door.origin}${door.mount}/api/chorus/domain/logs/gaps`, { timeout: 30_000 });
      expect(await fromServer(gaps), `the Gaps read did not reach the server behind the door: HTTP ${gaps.status()} ${(await gaps.text()).slice(0, 120)}`).toBe(true);
      // NEGATIVE: the same door, the same session, an address the list does not hold: the door's own refusal, never the server's answer
      const off = await context.request.get(`${door.origin}${door.mount}/api/chorus/card-story/1`, { timeout: 30_000, maxRedirects: 0 });
      expect(await fromServer(off), `an address off the allow list reached the server: HTTP ${off.status()} ${(await off.text()).slice(0, 120)}`).toBe(false);
      expect(await refusedByDoor(off), `the door's refusal reads: HTTP ${off.status()} ${(await off.text()).slice(0, 160)}`).toBe(true);
    } finally { await context.close(); }
  });

  test('NEGATIVE: a part pushed wider than the phone, a part left unread and a part taken out are each named', async ({ browser }) => {
    test.setTimeout(300_000);
    const { page, context } = await signedInPage(browser, door, PHONE);
    try {
      await serveOwnGaps(page, { logs: 'down' });
      await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=logs`, { waitUntil: 'domcontentloaded' });
      await page.waitForFunction(() => document.querySelector('#code-slot details') && document.querySelector('#gaps-slot details') && document.querySelector('#facets-deps details'), null, { timeout: 180_000 });
      await page.evaluate(() => {
        const fold = (re) => [...document.querySelectorAll('#main details.fold')].find((f) => re.test(f.querySelector('summary').textContent));
        fold(/^\s*Design doc/).style.width = '900px';
        fold(/^\s*Dependencies/).remove();
      });
      const faults = partFaults(await page.evaluate(fourParts)).join('\n');
      expect(faults).toMatch(/^Design doc: runs off the phone/m);
      expect(faults).toMatch(/^Dependencies: not drawn$/m);
      expect(faults).toMatch(/^Gaps: could not read Gaps/m);
      expect(faults).toMatch(/^the page scrolls sideways by \d+px$/m);
    } finally { await context.close(); }
  });
});
