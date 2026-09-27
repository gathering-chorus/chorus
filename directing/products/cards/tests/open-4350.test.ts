// @test-type: unit — pure functions over column names; no board, no network
/**
 * #4350 — every open card is visible. Jeff, 2026-09-26: "i want to see any
 * open card regardless of bucket/column". Eight hand-typed column lists had no
 * Keep, so 8 of Wren's Keep cards vanished from `cards list` and lost their
 * chunk on a rebuild the same evening.
 */
import { isOpen, statusOrder, CLOSED } from '../src/open';

describe('#4350 one definition of open', () => {
  test('Keep, and a column nobody has named yet, are open', () => {
    expect(isOpen('Keep')).toBe(true);
    expect(isOpen('Zzz')).toBe(true);
    expect(isOpen('Later')).toBe(true);
  });

  test('only Done and Won\'t Do close a card', () => {
    expect([...CLOSED].sort()).toEqual(['Done', "Won't Do"]);
    expect(isOpen('Done')).toBe(false);
    expect(isOpen("Won't Do")).toBe(false);
  });

  test('NEGATIVE PROOF: the list order shows every open column present, including one it has never heard of', () => {
    const order = statusOrder(['Later', 'Keep', 'Zzz', 'Done', 'WIP']);
    expect(order).toEqual(['WIP', 'Keep', 'Later', 'Zzz']);
    expect(order).not.toContain('Done');
    // the old hand-typed list, for contrast: it drops Keep and Zzz
    const old = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Blocked', 'Next', 'Later'];
    expect(['Later', 'Keep', 'Zzz', 'WIP'].filter(s => old.includes(s))).not.toContain('Keep');
  });

  test('closed columns come last, only when asked for', () => {
    expect(statusOrder(['Done', 'Later', "Won't Do"], true)).toEqual(['Later', "Won't Do", 'Done']);
  });
});
