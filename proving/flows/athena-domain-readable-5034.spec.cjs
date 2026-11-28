// @test-type: e2e:ui — playwright: the Athena domain page's tables can be read on a phone
// @domain: tests
/**
 * #5034 AC1 — Jeff's phone, 2026-11-24 12:11: the Files, Code and API Contract lists on
 * /athena/domain.html?d=principles were cut to seven letters a row ("latform" forty times).
 * 54 policy checks said the domain was green. None of them looks at the page.
 *
 * What a reader needs at 390 px:
 *   1. the page itself never scrolls sideways;
 *   2. every table's own scroll box is as wide as the fold it sits in (a box squeezed
 *      beside a floated label is the cut Jeff saw);
 *   3. no cell hides its text behind a hover title: a phone has no hover.
 * The page's files are served from this tree (lib/athena-page-5034.cjs); data is live.
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-readable-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { DOMAIN, PHONE, openDomainPage } = require('./lib/athena-page-5034.cjs');

test.describe(`#5034 the ${DOMAIN} domain page reads on a phone`, () => {
  test('the page body never scrolls sideways at 390 px', async ({ page }) => {
    const { errors } = await openDomainPage(page, PHONE);
    const over = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(over, `the page is ${over}px wider than the screen`).toBeLessThanOrEqual(1);
    expect(errors, `page threw: ${errors.join('; ')}`).toEqual([]);
  });

  test('every table box is as wide as its fold, so its rows are not cut', async ({ page }) => {
    test.setTimeout(240_000);
    await openDomainPage(page, PHONE);
    // the Conformance fold draws its table only when the domain lint has answered (12 s alone,
    // over 60 s under load). Checked before that, its table is not there to measure: this test
    // passed for three hours while that table was squeezed to 105px (found 11-24 16:44).
    await page.waitForFunction(() => !/running the domain lint|reading the last check/.test(document.getElementById('conf-slot')?.textContent || ''), null, { timeout: 180_000 });
    await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
    const squeezed = await page.evaluate(() => [...document.querySelectorAll('details.fold')].flatMap((fold) => {
      const body = fold.querySelector(':scope > .body');
      if (!body) return [];
      const room = body.clientWidth - parseFloat(getComputedStyle(body).paddingLeft) - parseFloat(getComputedStyle(body).paddingRight);
      const name = (fold.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim();
      return [...body.querySelectorAll('.tbl, :scope > table')].filter((box) => box.clientWidth < room - 2)
        .map((box) => `${name}: table box ${Math.round(box.clientWidth)}px of ${Math.round(room)}px`);
    }));
    expect(squeezed, `tables squeezed narrower than their fold:\n${squeezed.join('\n')}`).toEqual([]);
  });

  test('no cell hides its text behind a hover title', async ({ page }) => {
    await openDomainPage(page, PHONE);
    const hidden = await page.evaluate(() => [...document.querySelectorAll('details.fold td [title]')]
      .filter((el) => el.getAttribute('title') && el.textContent.trim() !== el.getAttribute('title').trim())
      .map((el) => `${(el.closest('details')?.querySelector('summary')?.childNodes[0]?.textContent || '').trim()}: shows "${el.textContent.trim()}" for "${el.getAttribute('title')}"`));
    expect(hidden, `cells that show less than they hold:\n${hidden.join('\n')}`).toEqual([]);
  });

  test('NEGATIVE: the checks are red on a page built the old way', async ({ page }) => {
    // the two shapes Jeff's screenshots showed, drawn by hand so this proof does not depend on today's bug
    await page.setViewportSize(PHONE);
    await page.setContent(`<style>.src{float:right;width:240px}.tbl{overflow-x:auto;max-width:100%}.body{padding:0 16px}</style>
      <details class="fold" open><summary>Code (2) <span class="src">chorus:SourceFile · hasDomain = principles · graph</span></summary>
      <div class="body"><div class="tbl"><table><tr><td>platform/api/src/handlers/domain-conformance.ts</td></tr>
      <tr><td><span title="/loom/principles-list.html">…/principles-list.html</span></td></tr></table></div></div></details>`);
    const squeezed = await page.evaluate(() => {
      const body = document.querySelector('.body'); const box = document.querySelector('.tbl');
      return box.clientWidth < body.clientWidth - 32 - 2;
    });
    expect(squeezed, 'a table box beside a floated label must read as squeezed').toBe(true);
    const hidden = await page.evaluate(() => [...document.querySelectorAll('td [title]')].filter((el) => el.textContent.trim() !== el.getAttribute('title').trim()).length);
    expect(hidden, 'a hover-only path must be counted').toBe(1);
  });
});

// =============================================================================================
// #5041 AC3 — it reads on a phone.
//
// Jeff's screenshots, 2026-11-25 10:22 to 10:34 (read by Borg Queen and Wren): card ids drawn one
// character a line (#/5/0/2/1), the owner shown as "principal-wren", release times broken over
// three lines, file paths cut at the right edge. The fold's wrap-anywhere rule did it to every
// narrow column, and the owner was the row's raw id.
//
// At 390 px, through the real sign-in door, at both mounts, each read from the page as drawn
// (the lines a text is drawn on are counted from its own boxes, never from the markup):
//   1. a card id is one line, "#5021", and the last Cards column can be reached (it is inside the
//      table's own scroll box, and that box is inside the screen);
//   2. the owner tile reads the bare name ("wren"), not the row id ("principal-wren");
//   3. a release time is one line, in the reader's own time zone (the browser is put in Tokyo:
//      this machine sits in the zone the times are stored in, so here "local" and "as stored" agree);
//   4. a file path is one line, reached by scrolling its table inside the fold, never broken
//      mid-word, and the page body does not scroll sideways;
//   5. a verdict is one word on one line.
// NEGATIVE: the page Jeff walked (cut from git at #5040) is red on the card id, the owner, the
// release time and the path. It already passed two: the last Cards column (its table scrolled)
// and the verdict (#5040 fixed that one). The test says so, so a change in either is seen.
//
// Page code read as drawn: platform/api/public/athena/facets.js (the one-line columns, the time),
// platform/api/public/athena/athena-flow.js (ownerLocal, localTime),
// platform/api/public/athena/domain.html (the Code fold's paths, the stylesheet rule).
// =============================================================================================
const { BASE, startDoor, signedInPage, openOldDomainPage } = require('./lib/athena-page-5034.cjs');

const READER_ZONE = 'Asia/Tokyo';
const OPS_5041 = `/athena/domain.html?d=${encodeURIComponent(DOMAIN)}`;

/** Run in the page (every fold open): the five things, as drawn. */
function phoneReading() {
  const text = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
  const nameOf = (f) => (f.querySelector(':scope > summary')?.childNodes[0]?.textContent || '').trim().replace(/\s*\(.*$/, '');
  const fold = (name) => [...document.querySelectorAll('details.fold')].find((f) => nameOf(f) === name);
  // the lines a node's text is drawn on: its text boxes, grouped by where each starts down the page
  const lines = (el) => {
    const r = document.createRange(); r.selectNodeContents(el);
    const cs = getComputedStyle(el);
    const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
    const tops = [...r.getClientRects()].filter((b) => b.width > 0 && b.height > 0).map((b) => b.top).sort((a, b) => a - b);
    let n = 0, last = -Infinity;
    for (const t of tops) if (t - last > lh * 0.6) { n++; last = t; }
    return n;
  };
  // a cell can be reached when it is inside the screen, or inside a box that scrolls sideways to it and is itself inside the screen
  const reach = (cell) => {
    const c = cell.getBoundingClientRect();
    const box = cell.closest('.tbl');
    if (!box) return c.right <= window.innerWidth + 1 ? null : `runs ${Math.round(c.right - window.innerWidth)}px off the screen with no scroll box`;
    const b = box.getBoundingClientRect();
    if (b.right > window.innerWidth + 1) return `its scroll box runs ${Math.round(b.right - window.innerWidth)}px off the screen`;
    if (c.right <= b.right + 1) return null;
    if (!/auto|scroll/.test(getComputedStyle(box).overflowX)) return 'is wider than its box, which does not scroll';
    return c.right - b.left + box.scrollLeft <= box.scrollWidth + 1 ? null : 'is cut: wider than its box can scroll';
  };
  const column = (f, header) => {
    const table = f?.querySelector('table'); if (!table) return [];
    const i = [...table.querySelectorAll('th')].findIndex((th) => text(th).toLowerCase() === header.toLowerCase());
    return i < 0 ? [] : [...table.querySelectorAll('tr')].map((tr) => tr.cells[i]).filter((c) => c && c.tagName === 'TD');
  };
  const cards = fold('Cards');
  const lastCards = cards ? [...cards.querySelectorAll('tr')].map((tr) => tr.cells[tr.cells.length - 1]).filter((c) => c && c.tagName === 'TD') : [];
  const isPath = (t) => /^[\w.@-]+(\/[\w.@-]+)+\.\w+$/.test(t);
  return {
    cardIds: column(cards, 'id').map((c) => ({ text: text(c), lines: lines(c) })),
    lastCardsColumn: lastCards.map((c) => reach(c)).filter(Boolean),
    cardsRows: lastCards.length,
    owner: text([...document.querySelectorAll('.stats .stat')].find((s) => /^Owner/.test(text(s.querySelector('.l'))))?.querySelector('.v')),
    releaseTimes: column(fold('Releases'), 'timestamp').map((c) => ({ text: text(c), lines: lines(c) })),
    paths: [...document.querySelectorAll('details.fold td')].filter((c) => isPath(text(c))).map((c) => ({ fold: nameOf(c.closest('details.fold')), text: text(c), lines: lines(c), reach: reach(c) })),
    verdicts: [...document.querySelectorAll('#conf-slot tr[data-policy]')].map((tr) => ({ verdict: tr.dataset.verdict, text: text(tr.cells[0]), lines: lines(tr.cells[0]) })),
    // every cell of ten letters or more that is drawn narrower than 120 px over three lines or more:
    // a word a few letters a line (the Alerts name at 51 px by 437 px, the UI Pages route at 57 px, 11-25 12:34)
    squeezed: [...document.querySelectorAll('details.fold td')].filter((c) => text(c).length >= 10)
      .map((c) => ({ fold: nameOf(c.closest('details.fold')), text: text(c).slice(0, 40), width: Math.round(c.getBoundingClientRect().width), lines: lines(c) }))
      .filter((c) => c.width > 0 && c.width < 120 && c.lines >= 3),
    routes: [...document.querySelectorAll('details.fold')].filter((f) => nameOf(f) === 'UI Pages').flatMap((f) => column(f, 'route')).map((c) => ({ text: text(c), lines: lines(c) })),
    // #5044 — every table's scroll box: whether its table runs past the box (measured here, from
    // the box itself, not from the page's own mark) and whether a line the reader can see says so
    tables: [...document.querySelectorAll('details.fold .tbl')].filter((b) => b.clientWidth > 0).map((b) => {
      const cue = b.previousElementSibling && b.previousElementSibling.classList.contains('cut-cue') ? b.previousElementSibling : null;
      const box = cue && cue.getBoundingClientRect();
      const seen = !!cue && box.width > 0 && box.height > 0 && getComputedStyle(cue).visibility !== 'hidden' && box.right <= window.innerWidth + 1 && box.left >= -1;
      return { fold: nameOf(b.closest('details.fold')), over: b.scrollWidth - b.clientWidth, cue: seen, says: cue ? text(cue) : '' };
    }),
    bodyOver: document.documentElement.scrollWidth - window.innerWidth,
  };
}

/** The reader's own time for a stored one, as the page's localTime writes it. */
const inZone = (iso, timeZone) => new Date(iso).toLocaleString('en-US', { timeZone, month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).replace(/\s+/g, ' ');

/** What a phone reader cannot read, by which of the five it is: { cardId, lastCardsColumn, owner, releaseTime, path, verdict }. */
function phoneFaults(r, { ownedBy, releases, zone }) {
  const f = { cardId: [], lastCardsColumn: [], owner: [], releaseTime: [], path: [], verdict: [], squeezed: [], cue: [] };
  // #5044 — a table that runs past its box says so in words; one that fits says nothing
  for (const t of r.tables || []) {
    if (t.over > 1 && !t.cue) f.cue.push(`${t.fold}: its table runs ${t.over}px past the right edge and nothing says it scrolls`);
    if (t.over > 1 && t.cue && !/sideways/.test(t.says)) f.cue.push(`${t.fold}: the line over its cut table does not say how to reach the rest ("${t.says}")`);
    if (t.over <= 1 && t.cue) f.cue.push(`${t.fold}: says there is more to the right, and its table fits`);
  }
  for (const c of r.squeezed || []) f.squeezed.push(`${c.fold}: "${c.text}" is squeezed to ${c.width}px over ${c.lines} lines`);
  for (const c of r.routes || []) if (c.lines !== 1) f.squeezed.push(`UI Pages: the route ${c.text} is drawn on ${c.lines} lines`);
  if (!r.cardIds.length) f.cardId.push('the Cards fold lists no card: nothing to read');
  for (const c of r.cardIds) if (!/^#\d+$/.test(c.text) || c.lines !== 1) f.cardId.push(`"${c.text}" is drawn on ${c.lines} lines`);
  if (!r.cardsRows) f.lastCardsColumn.push('the Cards fold lists no card: nothing to read');
  for (const why of r.lastCardsColumn) f.lastCardsColumn.push(`the last Cards column ${why}`);
  // the bare name: what is left of the row id after its last hyphenated prefix ("principal-wren" -> "wren")
  const raw = String(ownedBy || '').split('#').pop();
  if (!/^[a-z]+$/.test(r.owner) || !(raw === r.owner || raw.endsWith('-' + r.owner))) f.owner.push(`the owner tile reads "${r.owner}", the row is owned by ${raw}`);
  if (!r.releaseTimes.length) f.releaseTime.push('the Releases fold lists no release: nothing to read');
  r.releaseTimes.forEach((t, i) => {
    const want = releases[i] ? inZone(releases[i].timestamp, zone) : null;
    if (t.lines !== 1) f.releaseTime.push(`"${t.text}" is drawn on ${t.lines} lines`);
    if (want !== t.text) f.releaseTime.push(`"${t.text}" is not the reader's own time for ${releases[i]?.timestamp} (in ${zone}: "${want}")`);
  });
  if (!r.paths.length) f.path.push('no fold lists a file path: nothing to read');
  for (const p of r.paths) {
    if (p.lines !== 1) f.path.push(`${p.fold}: ${p.text} is broken over ${p.lines} lines`);
    if (p.reach) f.path.push(`${p.fold}: ${p.text} ${p.reach}`);
  }
  if (r.bodyOver > 1) f.path.push(`the page body scrolls sideways by ${r.bodyOver}px`);
  if (!r.verdicts.length) f.verdict.push('the Conformance fold lists no verdict: nothing to read');
  for (const v of r.verdicts) if (v.lines !== 1 || (/^(pass|fail)$/.test(v.verdict) && !/^\S+$/.test(v.text))) f.verdict.push(`"${v.text}" is drawn on ${v.lines} lines`);
  return f;
}

/** What the page is held to, asked directly: who owns the Domain row, and the stored release times. */
async function phoneTruth(request) {
  const get = async (p) => { const r = await request.get(BASE + p, { timeout: 30_000 }); if (!r.ok()) throw new Error(`${p}: HTTP ${r.status()}`); return r.json(); };
  const row = ((await get('/owl/domains')).data || []).find((d) => d.name === DOMAIN) || {};
  const rel = await get(`/api/chorus/domain/${encodeURIComponent(DOMAIN)}/releases`);
  return { ownedBy: row.ownedBy || (row.links || {}).ownedBy, releases: (rel.data || rel).releases || [], zone: READER_ZONE };
}

const drawnAll = async (page) => {
  await page.waitForFunction(() => !/loading the materialized view|loading live from the model|running the domain lint|reading the last check/.test(document.body.innerText) && document.querySelector('#conf-slot details'), null, { timeout: 180_000 });
  await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
  // #5044 — the page measures its tables a moment after a fold opens (one pass, 60 ms later); the
  // reading waits a fixed second for that pass, and never for the mark itself: a page that failed
  // to mark a cut table must be READ as failing, not waited on
  await page.waitForTimeout(1000);
  return page.evaluate(phoneReading);
};

for (const [where, mount] of [['under /chorus (the public apex)', '/chorus'], ['at the root (the door\'s own host)', '']]) {
  test.describe(`#5041 the ${DOMAIN} ops view reads on a phone, through the sign-in door ${where}`, () => {
    // opened once: each open runs the domain lint on the server, and the door gives its upstream 30 s
    let door, context, reading, truth;
    test.beforeAll(async ({ browser }) => {
      test.setTimeout(420_000);
      door = await startDoor({ mount });
      let page;
      ({ page, context } = await signedInPage(browser, door, { ...PHONE, timezoneId: READER_ZONE }));
      truth = await phoneTruth(context.request);
      await page.goto(door.origin + door.mount + OPS_5041, { waitUntil: 'domcontentloaded' });
      await page.waitForSelector('#code-slot details', { timeout: 120_000 });
      reading = await drawnAll(page);
    });
    test.afterAll(async () => { if (context) await context.close(); if (door) door.stop(); });
    const faults = () => phoneFaults(reading, truth);

    test('a card id is one line, and the last Cards column can be reached', () => {
      expect(faults().cardId, `card ids a phone cannot read:\n${faults().cardId.join('\n')}`).toEqual([]);
      expect(faults().lastCardsColumn, faults().lastCardsColumn.join('\n')).toEqual([]);
    });
    test('the owner reads the bare name, not the row id', () => {
      expect(faults().owner, faults().owner.join('\n')).toEqual([]);
    });
    test('a release time is one line, in the reader\'s own time zone', () => {
      expect(faults().releaseTime, `release times a phone cannot read:\n${faults().releaseTime.join('\n')}`).toEqual([]);
    });
    test('a file path is one line that scrolls inside its table, and the page body does not scroll sideways', () => {
      expect(reading.paths.some((p) => p.fold === 'Code'), 'the Code fold lists its files').toBe(true);
      expect(faults().path, `paths a phone cannot read:\n${faults().path.join('\n')}`).toEqual([]);
    });
    test('a verdict is one word on one line', () => {
      expect(faults().verdict, faults().verdict.join('\n')).toEqual([]);
    });
    // Borg Queen, navigating, at 390 px on the demo (11-25 12:34): the first build made the paths one
    // line and left the prose cell beside them 51 px wide, and the new UI Pages links five letters a line
    test('no cell is squeezed to a few letters a line, and every UI Pages route is one line', () => {
      expect(reading.routes.length, 'the UI Pages fold lists its routes').toBeGreaterThan(0);
      expect(faults().squeezed, `cells a phone cannot read:\n${faults().squeezed.join('\n')}`).toEqual([]);
    });
    // #5044 — Jeff's phone, 2026-11-25 15:43 to 15:45: paths, release titles and the Pipeline summary
    // ran off the right edge of five folds. Each table scrolled, and nothing said so.
    test('#5044 a table wider than the phone says it scrolls, and one that fits says nothing', () => {
      const cut = reading.tables.filter((t) => t.over > 1).map((t) => t.fold);
      expect(cut.length, `at 390 px some fold tables are wider than the screen (measured: ${JSON.stringify(reading.tables)})`).toBeGreaterThan(0);
      expect(faults().cue, `tables that are cut without saying so, or say so and are not:\n${faults().cue.join('\n')}`).toEqual([]);
      // the Pipeline table is short enough to fit a phone: it is not cut at all, and its summary wraps
      // inside the fold (read from the page at 19:47: the summary ran off the edge on one line)
      const pipeline = reading.tables.find((t) => t.fold === 'Pipeline');
      expect(pipeline, 'the Pipeline fold draws its table').toBeTruthy();
      expect(pipeline.over, `the Pipeline table runs ${pipeline.over}px past the fold`).toBeLessThanOrEqual(1);
    });
  });
}

// #5044 AC5 NEGATIVE — the same reading on the page with its cue taken away is red on each cut
// fold by name, and a cue under a table that fits is named too (drawn by hand).
test.describe('#5044 NEGATIVE: a cut table with no cue, and a cue with no cut, are each named', () => {
  test('the ops view without the cue is red on the folds Jeff photographed; with it, green', async ({ page }) => {
    test.setTimeout(300_000);
    await openDomainPage(page, PHONE);
    const read = async () => { await page.waitForTimeout(1000); return phoneFaults(await page.evaluate(phoneReading), { ownedBy: '', releases: [], zone: READER_ZONE }).cue; };
    expect(await read(), 'the page as built').toEqual([]);
    await page.addStyleTag({ content: '.cut-cue{display:none!important}' });
    const without = (await read()).join('\n');
    for (const fold of ['API Contract', 'UI Pages', 'Alerts', 'Releases']) expect(without, `without the cue the ${fold} fold must be named`).toMatch(new RegExp(`^${fold}: its table runs \\d+px past the right edge and nothing says it scrolls$`, 'm'));
    // and the Pipeline table without its fit mark is cut too, so "it fits" above can fail
    expect(without, 'the Pipeline table fits, so it is not named').not.toMatch(/^Pipeline: /m);
    await page.evaluate(() => document.querySelectorAll('.tbl table.fit').forEach((t) => t.classList.remove('fit')));
    expect((await read()).join('\n'), 'without its fit mark the Pipeline table is cut').toMatch(/^Pipeline: its table runs \d+px past the right edge and nothing says it scrolls$/m);
  });

  test('by hand: a cue under a table that fits, and a cue that does not say how, are named; a cut table with a true cue is not', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await page.setContent(`<style>body{margin:0;font:14px/20px sans-serif}.tbl{max-width:100%;overflow-x:auto}</style>
      <details class="fold" open><summary>Cards (1) </summary><div class="body"><p class="cut-cue">more to the right: slide the table sideways</p><div class="tbl"><table style="width:200px"><tr><td>fits</td></tr></table></div></div></details>
      <details class="fold" open><summary>Alerts (1) </summary><div class="body"><p class="cut-cue">more</p><div class="tbl"><table style="width:700px"><tr><td>wide</td></tr></table></div></div></details>
      <details class="fold" open><summary>Releases (1) </summary><div class="body"><p class="cut-cue">more to the right: slide the table sideways</p><div class="tbl"><table style="width:700px"><tr><td>wide</td></tr></table></div></div></details>
      <details class="fold" open><summary>Logs (1) </summary><div class="body"><div class="tbl"><table style="width:700px"><tr><td>wide</td></tr></table></div></div></details>`);
    const cue = phoneFaults(await page.evaluate(phoneReading), { ownedBy: '', releases: [], zone: READER_ZONE }).cue;
    expect(cue).toEqual([
      'Cards: says there is more to the right, and its table fits',
      'Alerts: the line over its cut table does not say how to reach the rest ("more")',
      'Logs: its table runs 310px past the right edge and nothing says it scrolls',
    ]);
  });
});

// The service page loads the same fold renderer, so it gets the same one-line cells. Its tables
// must scroll inside their folds too, or the whole page scrolls sideways (Borg Queen, 11-25 12:40:
// the scroll-box rule was on domain.html alone; it is in athena-flow.css now).
test.describe('#5041 the service page, which shares the fold renderer, reads on a phone too', () => {
  const { serveOwnAthenaFiles } = require('./lib/athena-page-5034.cjs');
  const open = async (page) => {
    await page.setViewportSize(PHONE);
    await serveOwnAthenaFiles(page);
    await page.goto(`${BASE}/athena/service.html?s=athena`, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => document.querySelectorAll('td.one').length > 0, null, { timeout: 120_000 });
    await page.evaluate(() => document.querySelectorAll('details').forEach((d) => { d.open = true; }));
  };
  const over = (page) => page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

  test('one-line cells are drawn, and the page body does not scroll sideways at 390 px', async ({ page }) => {
    test.setTimeout(180_000);
    await open(page);
    expect(await over(page), 'the service page is wider than the screen').toBeLessThanOrEqual(1);
    // #5044 — the cue is the shared file's (athena-flow.js), so this page's cut tables say they scroll too
    await page.waitForTimeout(1000);
    const cue = phoneFaults(await page.evaluate(phoneReading), { ownedBy: '', releases: [], zone: READER_ZONE }).cue;
    expect(cue, `service page tables that are cut without saying so:\n${cue.join('\n')}`).toEqual([]);
    // NEGATIVE: with the scroll boxes taken away the same page does scroll sideways, so the check can fail
    await page.addStyleTag({ content: '.tbl{overflow-x:visible!important;max-width:none!important}' });
    expect(await over(page), 'without its scroll boxes the page must run off the screen').toBeGreaterThan(1);
  });
});

test.describe('#5041 NEGATIVE: the phone checks are red on the page Jeff walked', () => {
  test('the ops view as it was at #5040: the card id, the owner, the release time and the path are each named', async ({ browser, request }) => {
    test.setTimeout(420_000);
    const door = await startDoor({ mount: '/chorus' });
    try {
      const truth = await phoneTruth(request);
      const { page, context } = await signedInPage(browser, door, { ...PHONE, timezoneId: READER_ZONE });
      // the policies answer is written here (one green, one red): this proof is about how the old
      // page DREW a verdict, and run live the lint can outlast the 30 s the door gives its upstream
      await page.route((url) => url.pathname === `${door.mount}/api/chorus/policies/counts`, (route) => route.fulfill({ status: 200, contentType: 'application/json',
        body: JSON.stringify({ measuredAt: '2026-11-25T16:00:00.000Z', policies: [['arch-domain-directory-layout', 'pass'], ['domain-folds-filled', 'fail']].map(([name, verdict]) => ({ name, label: name, verdict, count: 0, baseline: 0, scope: 'global' })) }) }));
      await openOldDomainPage(page, door.origin + door.mount);
      const f = phoneFaults(await drawnAll(page), truth);
      await context.close();
      expect(f.cardId.join('\n'), 'a card id one character a line').toMatch(/^"#\d+" is drawn on [2-9] lines$/m);
      expect(f.owner.join('\n'), 'the raw row id').toMatch(/^the owner tile reads "principal-\w+"/);
      expect(f.releaseTime.join('\n'), 'a stored time broken over lines').toMatch(/^"\d{4}-\d\d-\d\dT[\d:+-]+" is drawn on [2-9] lines$/m);
      expect(f.releaseTime.join('\n'), 'and not in the reader\'s zone').toMatch(/is not the reader's own time/);
      expect(f.path.join('\n'), 'a path broken mid-word').toMatch(/^\w[\w &]*: \S+ is broken over [2-9] lines$/m);
      // the two the old page already passed. Named, so this proof says what it does not show.
      expect(f.lastCardsColumn, 'the old Cards table already scrolled inside its fold').toEqual([]);
      expect(f.verdict, '#5040 had already made the verdict one word').toEqual([]);
    } finally { door.stop(); }
  });

  test('a cut last column, a wrapped verdict and a sideways page are each named (drawn by hand: the old page had none of them)', async ({ page }) => {
    await page.setViewportSize(PHONE);
    await page.setContent(`<style>body{margin:0;font:14px/20px sans-serif}td{padding:4px}.tbl{max-width:100%;overflow-x:hidden}.wide{width:700px}</style>
      <div class="stats"><div class="stat"><div class="v">wren</div><div class="l">Owner (ownedBy)</div></div></div>
      <details class="fold" open><summary>Cards (1) </summary><div class="body"><div class="tbl"><table style="width:600px"><tr><th>id</th><th>title</th><th>priority</th></tr><tr><td>#5021</td><td>t</td><td>P2</td></tr></table></div></div></details>
      <div id="conf-slot"><table style="width:60px"><tr data-policy="p" data-verdict="pass"><td><div style="width:20px;overflow-wrap:anywhere"><span>green</span></div></td></tr></table></div>
      <details class="fold" open><summary>Alerts (1) </summary><div class="body"><div class="tbl"><table style="width:340px"><tr><th>name</th><th>file</th></tr><tr><td style="overflow-wrap:anywhere">unreachable-principles-api</td><td style="white-space:nowrap">a one-line value that takes all the width there is</td></tr></table></div></div></details>
      <details class="fold" open><summary>UI Pages (1) </summary><div class="body"><div class="tbl"><table style="width:340px"><tr><th>route</th><th>pageType</th></tr><tr><td style="overflow-wrap:anywhere;width:50px"><a href="#">/loom/policy-counts.html</a></td><td>a page of this kind</td></tr></table></div></div></details>
      <div class="wide">x</div>`);
    const f = phoneFaults(await page.evaluate(phoneReading), { ownedBy: 'principal-wren', releases: [], zone: READER_ZONE });
    expect(f.squeezed.join('\n'), 'a name a few letters a line beside a one-line cell').toMatch(/^Alerts: "unreachable-principles-api" is squeezed to \d+px over [3-9]\d* lines$/m);
    expect(f.squeezed.join('\n'), 'a route broken over lines').toMatch(/^UI Pages: the route \/loom\/policy-counts\.html is drawn on [2-9] lines$/m);
    expect(f.squeezed.join('\n'), 'the wide one-line cell beside it is not named').not.toMatch(/a one-line value/);
    expect(f.lastCardsColumn.join('\n')).toMatch(/the last Cards column is wider than its box, which does not scroll/);
    expect(f.verdict.join('\n')).toMatch(/^"green" is drawn on [2-9] lines$/);
    expect(f.path.join('\n')).toMatch(/the page body scrolls sideways by \d+px/);
    expect(f.cardId, 'and a one-line card id on the same page is not named').toEqual([]);
    expect(f.owner, 'nor a bare owner').toEqual([]);
  });
});

// =============================================================================================
// #5045 AC5 — the four parts read on a phone, through the real sign-in door.
// In this file, in its own block, and not a file of its own: the layout rule counts every file
// under proving/flows (arch-domain-directory-layout; four new files took it from 1232 to 1236 on
// cicd run 4 of #5045). The block has its own names; nothing above is changed by it.
// =============================================================================================
{
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
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-readable-5034.spec.cjs -g "#5045"
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
}

// =============================================================================================
// #5042 AC5 — Jeff, 2026-11-27 16:26, after tapping API Contract opened the document itself:
// "This is not open api to me… giving me a link to json is crap". Tapping the fold's link opens
// a page he can read on his phone: the API's title, then each route with its method, drawn by
// swagger-ui-express (Gathering's viewer at /api-docs) from the document the generated API
// serves. Read-only: nothing on it sends a request ("Try it out" is off). Through the sign-in
// door, so the viewer's own files pass the door's allow-list; for principles (principles-api)
// and decisions (athena-make).
// NEGATIVE: the raw document is red, named; the same page with "Try it out" on is red, named.
{
  const { startDoor, signedInPage, PHONE, serveOwnApiDocs, ownApiDocs, LIVE_DISCOVERY: LIVE } = require('./lib/athena-page-5034.cjs');

  /** What the reader sees on the opened page, its first route opened as a tap would open it. */
  async function readApiPage(page) {
    await page.waitForFunction(() => document.contentType !== 'text/html' || document.querySelector('.swagger-ui .opblock, .api-docs-refused'), null, { timeout: 60_000 });
    const first = page.locator('.swagger-ui .opblock-summary').first();
    if (await first.count()) { await first.click(); await page.locator('.swagger-ui .opblock.is-open .opblock-section-header').first().waitFor({ timeout: 10_000 }); }
    return page.evaluate(() => {
      const clean = (n) => (n?.textContent || '').replace(/\s+/g, ' ').trim();
      return {
        type: document.contentType,
        title: clean(document.querySelector('.swagger-ui .info .title')),
        routes: [...document.querySelectorAll('.swagger-ui .opblock')].map((o) => `${clean(o.querySelector('.opblock-summary-method'))} ${clean(o.querySelector('.opblock-summary-path'))}`),
        // a button that would send: "Try it out", or "Execute" once trying is on
        tryItOut: [...document.querySelectorAll('.swagger-ui button')].some((b) => /try it out|execute/i.test(b.textContent)),
        sideways: document.documentElement.scrollWidth - window.innerWidth,
      };
    });
  }

  /** What is wrong with the page a reader landed on. */
  function apiPageFaults(p) {
    if (p.type !== 'text/html') return [`the link opens the raw document (${p.type}), not a page`];
    const f = [];
    if (!p.title) f.push('the page names no API title');
    if (!p.routes.length) f.push('the page draws no route');
    if (p.routes.some((r) => !/^(GET|POST|PUT|PATCH|DELETE) \//.test(r))) f.push(`a route is drawn without its method: ${p.routes.join(', ')}`);
    if (p.tryItOut) f.push('the page offers "Try it out": it can send a request');
    if (p.sideways > 1) f.push(`the page scrolls sideways by ${p.sideways}px at phone width`);
    return f;
  }

  async function openFromFold(page, door, domain) {
    await page.goto(`${door.origin}${door.mount}/athena/domain.html?d=${domain}`, { waitUntil: 'domcontentloaded' });
    const link = page.locator('#main details.fold a[data-openapi]').first();
    await link.waitFor({ state: 'attached', timeout: 180_000 });
    const href = await link.getAttribute('href');
    await page.goto(new URL(href, page.url()).href, { waitUntil: 'domcontentloaded' });
    return href;
  }

  test.describe('#5042 AC5 tapping API Contract opens a readable, read-only API page on a phone', () => {
    test.setTimeout(420_000);
    let door;
    test.beforeAll(async () => { door = await startDoor({ mount: '/chorus' }); });
    test.afterAll(async () => { if (door) door.stop(); });

    for (const domain of ['principles', 'decisions']) {
      test(`${domain}: the link opens a drawn page with its routes and nothing that sends`, async ({ browser }) => {
        const { page, context } = await signedInPage(browser, door, PHONE);
        try {
          await serveOwnApiDocs(page);
          const href = await openFromFold(page, door, domain);
          expect(href, 'the link stays under the door').toMatch(/\/athena\/api-docs\/[a-z0-9-]+\/[a-z0-9-]+\/$/);
          const p = await readApiPage(page);
          const faults = apiPageFaults(p);
          expect(faults, `${page.url()}\n${faults.join('\n')}`).toEqual([]);
        } finally { await context.close(); }
      });
    }

    test('the door passes the page and its viewer files (the allow-list holds /athena); an address off it is refused', async ({ browser }) => {
      const { context } = await signedInPage(browser, door, PHONE);
      try {
        const refusedByDoor = async (r) => r.status() === 404 && /^path not shared\b/.test((await r.text()).trim());   // the door's own words for an address off its list
        const at = `${door.origin}${door.mount}/athena/api-docs/principles/principles/`;
        for (const f of ['', 'swagger-ui.css', 'swagger-ui-bundle.js', 'swagger-ui-standalone-preset.js', 'swagger-ui-init.js']) {
          const r = await context.request.get(at + f, { timeout: 30_000, maxRedirects: 0 });
          expect(await refusedByDoor(r), `the door refused ${at + f}`).toBe(false);
        }
        const off = await context.request.get(`${door.origin}${door.mount}/api-docs/principles/principles/`, { timeout: 30_000, maxRedirects: 0 });
        expect(await refusedByDoor(off), `an address off the list was not refused: HTTP ${off.status()}`).toBe(true);
      } finally { await context.close(); }
    });

    test('NEGATIVE: the raw document, and the same page with "Try it out" on, are each named', async ({ browser }) => {
      const { page, context } = await signedInPage(browser, door, PHONE);
      try {
        await serveOwnApiDocs(page);
        const raw = `${door.origin}${door.mount}/owl/v1/principles/principles/openapi.json`;
        await page.goto(raw, { waitUntil: 'domcontentloaded' });
        expect(apiPageFaults(await readApiPage(page)).join('\n')).toMatch(/opens the raw document/);
        // the viewer's own init script, with submitting turned back on: what the page would be without the setting
        await page.route((u) => u.pathname.endsWith('/athena/api-docs/principles/principles/swagger-ui-init.js'), async (route) => {
          // the page's own answer (this tree's before the land, the live door's after it)
          const r = LIVE ? await route.fetch().then(async (x) => ({ status: x.status(), headers: x.headers(), body: await x.body() })) : await ownApiDocs(route.request().url());
          const body = r.body.toString().replace('"supportedSubmitMethods": []', '"supportedSubmitMethods": ["get","post","put","patch","delete"]');
          await route.fulfill({ status: r.status, headers: r.headers, body });
        });
        await page.goto(`${door.origin}${door.mount}/athena/api-docs/principles/principles/`, { waitUntil: 'domcontentloaded' });
        expect(apiPageFaults(await readApiPage(page)).join('\n')).toMatch(/offers "Try it out"/);
        // and a link to an API page whose document does not exist leads nowhere (what the link checks read before the land)
        const nowhere = LIVE ? (await context.request.get(`${door.origin}${door.mount}/athena/api-docs/principles/nosuch/`)).status() : (await ownApiDocs(`${door.origin}${door.mount}/athena/api-docs/principles/nosuch/`)).status;
        expect(nowhere, 'an API page with no document answered as if it led somewhere').toBe(404);
      } finally { await context.close(); }
    });
  });
}
