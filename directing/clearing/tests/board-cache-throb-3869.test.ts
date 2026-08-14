// @test-type: unit — pure merge over cache inputs, no io.
/**
 * #3869 (second leg) — the role cards "throb".
 *
 * Jeff, 2026-08-14: "on the role cards the state throbs — it will show your wip
 * cards then the wip cards disappear then they return."
 *
 * `refreshBoardFromApi` polls two endpoints with allSettled. A rejected fetch,
 * a non-ok response, or a body missing `data.cards` all collapse to `null`,
 * then `?? []` turns that into an empty list, and the cache is REPLACED with
 * it. So one failed poll blanks every card on screen until the next good one
 * lands — which is the flicker, exactly on the polling interval.
 *
 * The bug is that a failed read and a genuinely empty board produce the same
 * value. They are different facts: "I could not ask" is not "there is nothing."
 *
 * NEGATIVE PROOF (#3734): the first case fails against today's code, which
 * clobbers. The `genuinely empty` case is the other half — a fix of "never
 * replace with empty" would pass the first and leave a finished card on screen
 * forever, which is a different lie in the same place.
 */

import { mergeBoardRefresh } from '../src/tiles';

const CARD = { id: 3869, owner: 'Wren', title: 'x' };
const prev = { wip_cards: [CARD], swat_cards: [], ts: 1000 };

describe('#3869 — a failed poll must not blank the board', () => {
  it('keeps the previous cards when the WIP read failed', () => {
    const next = mergeBoardRefresh(prev, { wip: null, swat: [] }, 2000);
    expect(next.wip_cards).toEqual([CARD]);
  });

  it('keeps the previous cards when both reads failed', () => {
    const next = mergeBoardRefresh(prev, { wip: null, swat: null }, 2000);
    expect(next.wip_cards).toEqual([CARD]);
    expect(next.ts).toBe(prev.ts); // nothing was learned, so nothing is newer
  });

  it('DOES clear when the board genuinely reports no WIP cards', () => {
    const next = mergeBoardRefresh(prev, { wip: [], swat: [] }, 2000);
    expect(next.wip_cards).toEqual([]);
    expect(next.ts).toBe(2000);
  });

  it('replaces cards on a successful read', () => {
    const other = { id: 3870, owner: 'Wren', title: 'y' };
    const next = mergeBoardRefresh(prev, { wip: [other], swat: [] }, 2000);
    expect(next.wip_cards).toEqual([other]);
  });

  it('a failed swat read does not discard a good wip read', () => {
    const next = mergeBoardRefresh(prev, { wip: [CARD], swat: null }, 2000);
    expect(next.wip_cards).toEqual([CARD]);
    expect(next.swat_cards).toEqual(prev.swat_cards);
  });
});
