// @test-type: unit — pure sequence arithmetic; no relay, no clock.
// @domain: messages
// @card: #4445
// @owner: wren
/**
 * #4445 — Jeff's Clearing, 2026-10-08 07:38: "66 messages from abby-normal never
 * arrived (#0, 1, 2, 3, 4 … +61 more)". Abby's replies reach the room now, but
 * she published 66 before the room listened for replies. Those were never this
 * room's to miss. Holes are counted from the first number the room saw.
 */
import { emptySeqState, holesFor, recordSeq } from '../src/room-sequence';

describe('#4445 holes count from the first note the room saw', () => {
  test('an author first seen at #66 reports no holes', () => {
    const st = emptySeqState();
    recordSeq(st, 'abby-normal', 66);
    expect(holesFor(st, 'abby-normal')).toEqual([]);
  });

  test('NEGATIVE PROOF: a gap after the first note seen is still reported', () => {
    const st = emptySeqState();
    for (const n of [66, 67, 70]) recordSeq(st, 'abby-normal', n);
    expect(holesFor(st, 'abby-normal')).toEqual([68, 69]);
  });

  test('a late lower note widens the range it is checked from', () => {
    const st = emptySeqState();
    for (const n of [66, 64]) recordSeq(st, 'abby-normal', n);
    expect(holesFor(st, 'abby-normal')).toEqual([65]);
  });
});
