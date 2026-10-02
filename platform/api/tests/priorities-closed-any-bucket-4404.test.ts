// @test-type: unit — pure function over fixture bucket rows; no DB, no HTTP.
// @card: #4404
// @owner: wren
/**
 * #4404 — /sup listed closed cards. A Vikunja card sits in one bucket per view:
 * a Done card is in "Done" on the kanban view AND in "To-Do" on another view.
 * The status read joined every bucket and kept whichever row came last, so a
 * Done card read as "To-Do" and stayed in the walk.
 */
import { statusesFromBucketRows } from '../src/handlers/context-priorities';

describe('#4404 a card closed on any view is closed', () => {
  test('Done + To-Do reads Done, whichever row comes first', () => {
    expect(statusesFromBucketRows([{ id: 3839, status: 'Done' }, { id: 3839, status: 'To-Do' }]).get(3839)).toBe('Done');
    expect(statusesFromBucketRows([{ id: 3839, status: 'To-Do' }, { id: 3839, status: 'Done' }]).get(3839)).toBe('Done');
  });

  test("Won't Do + To-Do reads Won't Do", () => {
    expect(statusesFromBucketRows([{ id: 3491, status: 'To-Do' }, { id: 3491, status: "Won't Do" }]).get(3491)).toBe("Won't Do");
  });

  test('NEGATIVE PROOF: an open card in two open buckets stays open; a card only in Next stays', () => {
    const m = statusesFromBucketRows([
      { id: 4217, status: 'Keep' }, { id: 4217, status: 'To-Do' },
      { id: 4049, status: 'Next' },
      { id: 9, status: null },
    ]);
    expect(m.get(4217)).toBe('Keep');
    expect(m.get(4049)).toBe('Next');
    expect(m.has(9)).toBe(false);
  });
});
