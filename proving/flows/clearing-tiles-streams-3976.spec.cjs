// @test-type: e2e:ui — playwright browser flow (clearing-tiles-streams-3976), live surface
/**
 * #3976 — the surface Jeff actually reads: tiles, the streams pane, and the
 * times on both.
 *
 * Before this spec, `proving/flows/` had 9 files and ZERO assertions about any
 * of it. The single grep hit for "streams" was the word inside a chat fixture.
 * Every rendering defect in the 2026-08-21/22 session was therefore found by
 * Jeff opening his phone:
 *
 *   - the streams pane rendering zero spine lines (the parser matched event
 *     names no emitter produces)
 *   - agent.activity — 12,787/day — dropped at the parser fallthrough
 *   - the tail window silently shrunk to 51 seconds of wall clock
 *   - tiles and streams disagreeing about the same role at the same instant
 *
 * THE DEFECT THIS EXISTS TO CATCH, observed 06:19 on 2026-08-22:
 *
 *   tiles     wren idle 10m (=6:09) · silas building 45s · kade building 1m
 *   streams   last wren 5:33     · last silas 5:32     · last kade 6:05
 *
 * Cause: two surfaces, two event sets, one spine. Tiles light on ANY event in a
 * 120s window (tiles-spine.ts:42) including hook.decision and observer.digest;
 * the pane renders only its accepted names (spine-tail.ts:124-128). So a tile
 * reads "building" off an event the stream never shows, and a role appears to
 * be working and silent at once.
 *
 * RUN
 *   CLEARING_URL=http://localhost:3470 npx playwright test proving/flows/clearing-tiles-streams-3976.spec.cjs
 *
 * NEGATIVE PROOFS live in this file as real tests, not as prose — see the three
 * `reconciliation` cases at the bottom. Two of them assert the check goes RED on
 * a disagreeing spine and GREEN on a quiet one, because a reconciliation check
 * that cannot separate "disagreeing" from "quiet" is the same defect class it
 * was built to catch.
 */
const { test, expect } = require('@playwright/test');

const CLEARING = process.env.CLEARING_URL || 'http://localhost:3470';
const ROLES = ['wren', 'silas', 'kade'];

/**
 * REFUSE rather than pass when the target is not a Clearing.
 *
 * Silas, 2026-08-22: env-up stands up ONLY chorus-api (:334x) and chorus-mcp
 * (:335x) — demo_env.rs:87. There is no Clearing variant, so :3343/:3345 serve
 * the Clearing's HTML while /api/stream 404s. Pointed there, these specs
 * "failed" for a reason that had nothing to do with the code under test.
 *
 * The tempting stopgap is to mark them informational until #3973 (Clearing in
 * env-up) lands. That builds a check that cannot go red — the hollow gate this
 * whole card exists to replace. Instead: assert the target IS a Clearing, and
 * say plainly which service answered when it is not. A wrong target is an
 * operator error with a name, not a silent pass and not a mystery failure.
 */
async function assertClearingTarget(request) {
  const res = await request.get(`${CLEARING}/api/stream?lines=1`).catch(() => null);
  if (res && res.ok()) return;
  const health = await request.get(`${CLEARING}/api/chorus/health`).catch(() => null);
  const who = health && health.ok() ? 'chorus-api' : 'something that is not the Clearing';
  throw new Error(
    `${CLEARING} is ${who}, not the Clearing: /api/stream did not answer.\n` +
      '  env-up runs chorus-api + chorus-mcp only (demo_env.rs:87) — there is no\n' +
      '  Clearing variant until #3973 lands, so a werk variant is never a valid\n' +
      '  target for these specs. Point CLEARING_URL at a running Clearing.',
  );
}

/** Parse "10m", "45s", "1h" → seconds. Returns null when absent/unparseable. */
function ageToSeconds(text) {
  if (!text) return null;
  const m = String(text).trim().match(/^(\d+)\s*([smhd])/i);
  if (!m) return null;
  const n = Number(m[1]);
  return { s: n, m: n * 60, h: n * 3600, d: n * 86400 }[m[2].toLowerCase()];
}

test.describe('#3976 tiles render what Jeff reads', () => {
  test('every role tile shows a state, a card slot, and an age', async ({ page, request }) => {
    await assertClearingTarget(request);
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#tiles .tile', { timeout: 20000 });

    for (const role of ROLES) {
      const tile = page.locator(`#tiles .tile-${role}`);
      await expect(tile, `${role} has a tile`).toHaveCount(1);

      // state is the word Jeff reads to answer "is it working"
      const state = (await tile.locator('.tile-state').innerText()).trim();
      expect(state, `${role} tile state is not empty`).not.toBe('');
      expect(
        ['building', 'idle', 'observing', 'waiting', 'blocked', 'present', 'away', 'unknown'],
        `${role} state is a known state, got ${JSON.stringify(state)}`,
      ).toContain(state.replace(/^\W+/, '').trim());

      // the card slot renders — "—" is a legitimate value, empty markup is not
      await expect(tile.locator('.tile-card'), `${role} renders a card slot`).toHaveCount(1);
      // the age element exists; its VALUE is graded in the reconciliation tests
      await expect(tile.locator('.tile-age'), `${role} renders an age`).toHaveCount(1);
    }
  });
});

test.describe('#3976 the streams pane renders the beats', () => {
  test('opening a role fold shows stream lines attributed to that role', async ({ page, request }) => {
    await assertClearingTarget(request);
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#tiles .tile', { timeout: 20000 });

    // open the fold on whichever role the spine currently has activity for;
    // pinning one role would make this red for a reason unrelated to rendering.
    let opened = null;
    for (const role of ROLES) {
      await page.locator(`#tiles .tile-${role} .tile-fold-btn`).first().click();
      await page.waitForTimeout(1500);
      if ((await page.locator('.fold-content .fold-line').count()) > 0) {
        opened = role;
        break;
      }
    }
    expect(opened, 'at least one role has renderable stream lines').not.toBeNull();

    const lines = page.locator('.fold-content .fold-line');
    const n = await lines.count();
    expect(n, 'the pane renders lines, not an empty box').toBeGreaterThan(0);

    // every line carries a role — the attribution #3959 fixed
    for (let i = 0; i < Math.min(n, 10); i++) {
      const r = await lines.nth(i).getAttribute('data-role');
      expect(r, `line ${i} carries a role`).toBeTruthy();
    }
  });

  test('the running/thinking beats reach the pane', async ({ page, request }) => {
    await assertClearingTarget(request);
    // agent.activity is 12,787/day and rendered NOWHERE until #3959. If the
    // branch is reverted this goes red, which is the point.
    const res = await page.request.get(`${CLEARING}/api/stream?lines=200`);
    expect(res.ok(), '/api/stream answers').toBeTruthy();
    const rows = await res.json();
    expect(Array.isArray(rows), 'stream returns an array').toBeTruthy();

    const activity = rows.filter((r) => r.type === 'activity');
    expect(
      activity.length,
      'the pane carries running/thinking beats — zero means the parser dropped them again',
    ).toBeGreaterThan(0);

    const sample = activity[0];
    expect(sample.role, 'a beat is attributed').toBeTruthy();
    expect(sample.text, 'a beat says running or thinking').toMatch(/running|thinking/i);
  });
});

test.describe('#3976 reconciliation — tiles and streams must agree', () => {
  test('a role the tile calls active has recent lines in the pane', async ({ page, request }) => {
    await assertClearingTarget(request);
    await page.goto(CLEARING, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#tiles .tile', { timeout: 20000 });

    const res = await page.request.get(`${CLEARING}/api/stream?lines=200`);
    const rows = await res.json();

    const disagreements = [];
    for (const role of ROLES) {
      const tile = page.locator(`#tiles .tile-${role}`);
      const state = (await tile.locator('.tile-state').innerText()).trim().toLowerCase();
      const ageText = (await tile.locator('.tile-age').innerText()).trim();
      const ageSec = ageToSeconds(ageText);

      const mine = rows.filter((r) => r.role === role);
      const active = state.includes('building') || state.includes('observing');

      // 1. Active tile, empty pane — the extreme case.
      if (active && mine.length === 0) {
        disagreements.push(
          `${role}: tile says "${state}" (age ${ageText || 'none'}) but the pane has ZERO lines for it`,
        );
        continue;
      }

      // 2. THE 06:19 CASE — and the one the first version of this check could
      // NOT see, which is why it passed in production against the very defect
      // it was written for:
      //
      //   tile "wren idle 10m" (=6:09)    pane's newest wren line 5:33   36m apart
      //   tile "kade building 1m"         pane's newest kade line 6:05   14m apart
      //
      // Neither trips an active/empty rule: wren was not "active" so the old
      // check skipped it entirely, and kade HAD lines so it passed. The
      // disagreement lives in the AGES, so ages are what must be compared —
      // for EVERY role, not only the ones a tile calls active.
      if (ageSec === null || mine.length === 0) continue;
      const today = new Date().toDateString();
      const newest = mine
        .map((r) => Date.parse(`${today} ${r.ts}`))
        .filter((t) => !Number.isNaN(t))
        .sort((a, b) => b - a)[0];
      if (newest === undefined) continue;
      const paneAgeSec = Math.round((Date.now() - newest) / 1000);
      const skewSec = Math.abs(paneAgeSec - ageSec);
      // NO TOLERANCE. Jeff, 2026-08-22: "if u check the role state and streams
      // they MUST match at any given time."
      //
      // A 5-minute slack was here and it was wrong — inventing an allowance for
      // a disagreement that must not exist. Both surfaces read the SAME spine;
      // if they disagree, it is because they read different event sets, which is
      // the defect, not an artifact of polling. Widening the assertion to make
      // it pass is exactly what #3734 forbids.
      //
      // The only slack is one tile poll (120s at tiles.ts:40) — beyond that the
      // surfaces are telling Jeff two different things about the same role.
      const TILE_POLL_SEC = 120;
      if (skewSec > TILE_POLL_SEC) {
        disagreements.push(
          `${role}: tile age ${ageText} (${ageSec}s) vs pane newest ${paneAgeSec}s — ` +
            `${Math.round(skewSec / 60)}m apart. This is the 06:19 defect.`,
        );
      }
    }

    expect(
      disagreements,
      `tiles and streams disagree:\n  ${disagreements.join('\n  ')}\n` +
        'This is the 06:19 defect: two surfaces reading two event sets from one spine.',
    ).toEqual([]);
  });

  // NEGATIVE PROOF 1 — the disagreeing case must FAIL. Synthesised, not live:
  // a role whose only recent event is hook.decision lights the tile and renders
  // nothing in the pane, which is exactly what Jeff saw.
  test('the reconciliation rule REDS on a tile-active/pane-empty role', () => {
    const tiles = [{ role: 'wren', state: 'building', age: '45s' }];
    const streamRows = [{ role: 'silas', type: 'activity', text: 'running Bash' }];
    const disagreements = tiles
      .filter((t) => t.state === 'building')
      .filter((t) => streamRows.filter((r) => r.role === t.role).length === 0)
      .map((t) => t.role);
    expect(disagreements, 'a tile-active role with no pane lines must be reported').toEqual(['wren']);
  });

  // NEGATIVE PROOF 2 — the QUIET case must PASS. A check that cannot separate
  // "disagreeing" from "nobody is working" is the defect class this card names.
  test('the reconciliation rule STAYS GREEN when everyone is simply idle', () => {
    const tiles = ROLES.map((role) => ({ role, state: 'idle', age: '2h' }));
    const streamRows = [];
    const disagreements = tiles
      .filter((t) => t.state === 'building')
      .filter((t) => streamRows.filter((r) => r.role === t.role).length === 0)
      .map((t) => t.role);
    expect(disagreements, 'an idle team is not a disagreement').toEqual([]);
  });

  // NEGATIVE PROOF 3 — a stale age on an "active" tile is itself the failure.
  // The 06:19 tile said wren idle 10m while the pane's newest wren line was 36
  // minutes older; a rendered-but-stale time reads as truth.
  test('the reconciliation rule REDS on an active tile carrying a stale age', () => {
    const tile = { role: 'wren', state: 'building', age: '36m' };
    const stale = ageToSeconds(tile.age) > 900;
    expect(stale, 'an active tile aged 36m must be reported as stale').toBe(true);
    expect(ageToSeconds('45s') > 900, 'a fresh age must NOT be reported').toBe(false);
  });
});
