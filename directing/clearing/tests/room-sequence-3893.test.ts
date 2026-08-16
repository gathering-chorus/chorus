// @test-type: unit — pure; brings its own world.
/**
 * #3893 — proofs that a missing message is VISIBLE.
 *
 * NEGATIVE PROOF (#3734): `a_dropped_message_is_reported_not_skipped` feeds the
 * detector a stream with 5 and 6 removed and requires it to name them. The
 * check it guards against is the one that would pass by simply rendering
 * whatever arrived — which every previous version of this room did, and which
 * is why Jeff spent days hearing sentences one, four and eight.
 */
import {
  allHoles,
  emptySeqState,
  HOLE_WINDOW,
  holeMarker,
  holesFor,
  recordSeq,
  seqOf,
} from '../src/room-sequence';

const ev = (seq: number | string | null) => ({
  tags: seq === null ? [['t', 'team']] : [['t', 'team'], ['seq', String(seq)]],
});

/** Feed a stream of (author, seq) into a fresh state. */
function ingest(stream: Array<[string, number]>) {
  const st = emptySeqState();
  for (const [author, seq] of stream) recordSeq(st, author, seq);
  return st;
}

describe('#3893 sequence — the room can count, so it can say what never came', () => {
  it('NEGATIVE: a dropped message is reported, not silently skipped', () => {
    // wren published 0..9. The relay delivered everything except 5 and 6.
    const delivered: Array<[string, number]> = [0, 1, 2, 3, 4, 7, 8, 9].map((n) => ['wren', n]);
    const st = ingest(delivered);

    expect(holesFor(st, 'wren')).toEqual([5, 6]);
    expect(holeMarker('wren', holesFor(st, 'wren')))
      .toBe('2 messages from wren never arrived (#5, 6)');
  });

  it('a complete conversation reports nothing — no crying wolf', () => {
    const st = ingest([0, 1, 2, 3, 4].map((n) => ['wren', n] as [string, number]));
    expect(holesFor(st, 'wren')).toEqual([]);
    expect(allHoles(st)).toEqual([]);
  });

  it('out-of-order delivery is not a hole', () => {
    const st = ingest([['wren', 3], ['wren', 0], ['wren', 2], ['wren', 1]]);
    expect(holesFor(st, 'wren')).toEqual([]);
  });

  it('a message still in flight is not a hole', () => {
    // We have seen 0..4. Nothing claims 5 exists yet, so 5 is not missing.
    const st = ingest([0, 1, 2, 3, 4].map((n) => ['wren', n] as [string, number]));
    expect(holesFor(st, 'wren')).not.toContain(5);
  });

  it('counts per author — silas losing one does not implicate kade', () => {
    const st = ingest([
      ['wren', 0], ['wren', 1],
      ['silas', 0], ['silas', 2],
      ['kade', 0], ['kade', 1], ['kade', 2],
    ]);
    expect(allHoles(st)).toEqual([{ author: 'silas', missing: [1] }]);
  });

  it('unnumbered notes render normally — a missing feature is not missing messages', () => {
    expect(seqOf(ev(null))).toBeNull();
    const st = emptySeqState();
    recordSeq(st, 'wren', seqOf(ev(null)));
    expect(allHoles(st)).toEqual([]);
  });

  it('a malformed seq tag is treated as absent, never as sequence zero', () => {
    expect(seqOf(ev('banana'))).toBeNull();
    expect(seqOf(ev('-1'))).toBeNull();
    expect(seqOf(ev('1.5'))).toBeNull();
    expect(seqOf(ev(0))).toBe(0);
  });

  it('holes older than the window stop being reported, and stop being stored', () => {
    const st = emptySeqState();
    recordSeq(st, 'wren', 0);                       // the ancient miss is #1
    for (let n = 2; n <= HOLE_WINDOW + 50; n++) recordSeq(st, 'wren', n);
    expect(holesFor(st, 'wren')).not.toContain(1);
    expect(st.seen.get('wren')!.size).toBeLessThanOrEqual(HOLE_WINDOW + 1);
  });

  it('the marker names the count and the numbers, and abbreviates a flood', () => {
    expect(holeMarker('kade', [7])).toBe('1 message from kade never arrived (#7)');
    expect(holeMarker('kade', [1, 2, 3, 4, 5, 6, 7]))
      .toBe('7 messages from kade never arrived (#1, 2, 3, 4, 5 … +2 more)');
    expect(holeMarker('kade', [])).toBe('');
  });
});
