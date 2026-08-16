// @test-type: unit — brings its own world (tmpdir cursor file), no live relay.
/**
 * #3893 — replay proofs.
 *
 * NEGATIVE PROOF (#3734): `the old limit:30 window loses notes the cursor keeps`
 * runs BOTH filters against the same 100-note gap — the old `limit: 30` sample
 * and the production `roomFilter` — and shows the old one dropping 70. Without
 * that side-by-side the other tests all pass on a filter that samples, because
 * "gave me some notes" and "gave me every note" look identical from inside.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import {
  advanceCursor,
  COLD_START_LIMIT,
  OVERLAP_SECS,
  readCursor,
  roomFilter,
  writeCursor,
} from '../src/room-replay';

/** The relay, reduced to the only behaviour under test: it honours the filter. */
function relayServes(notes: number[], filter: { since?: number; limit?: number }): number[] {
  let out = notes.slice().sort((a, b) => a - b);
  if (filter.since !== undefined) out = out.filter((t) => t >= filter.since!);
  if (filter.limit !== undefined) out = out.slice(-filter.limit);
  return out;
}

describe('#3893 replay — the subscription asks for everything it missed', () => {
  const T0 = 1_760_000_000;
  /** 100 notes said while we were away, one per second. */
  const gap = Array.from({ length: 100 }, (_, i) => T0 + i);
  /** The last note we rendered, just before the gap opened. */
  const cursor = T0 - 1;

  it('NEGATIVE: the old limit:30 window loses notes the cursor keeps', () => {
    const sampled = relayServes(gap, { limit: 30 });
    const replayed = relayServes(gap, roomFilter('team', cursor));

    // The old subscription. The 70 that never arrive are the ones Jeff never
    // sees, and nothing in the response says they existed. This is the shotgun.
    expect(sampled).toHaveLength(30);
    expect(sampled).not.toContain(gap[0]);

    // Same gap, same relay, production filter: nothing missing.
    expect(replayed).toHaveLength(100);
    expect(replayed).toEqual(expect.arrayContaining(sampled));
  });

  it('serves every note said during the disconnect, first to last', () => {
    const served = relayServes(gap, roomFilter('team', cursor));
    expect(served[0]).toBe(gap[0]);
    expect(served[99]).toBe(gap[99]);
  });

  it('a replay filter carries no limit — a bigger sample is still a sample', () => {
    expect(roomFilter('team', T0).limit).toBeUndefined();
    expect(roomFilter('team', T0).since).toBe(T0 - OVERLAP_SECS);
  });

  it('rewinds by the overlap so a same-second note is not lost to a strict boundary', () => {
    const served = relayServes([T0], roomFilter('team', T0));
    expect(served).toEqual([T0]);
  });

  it('cold start takes a bounded backfill, not an unbounded history', () => {
    const f = roomFilter('team', null);
    expect(f.limit).toBe(COLD_START_LIMIT);
    expect(f.since).toBeUndefined();
  });

  it('the cursor only moves forward, whatever order the relay replays in', () => {
    let c: number | null = null;
    for (const t of [T0 + 5, T0 + 1, T0 + 9, T0 + 2]) c = advanceCursor(c, t);
    expect(c).toBe(T0 + 9);
  });

  it('a garbage timestamp does not move the cursor', () => {
    expect(advanceCursor(T0, NaN)).toBe(T0);
    expect(advanceCursor(T0, 0)).toBe(T0);
    expect(advanceCursor(T0, -1)).toBe(T0);
  });

  describe('persistence', () => {
    let dir: string;
    beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'room-replay-')); });
    afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

    it('survives a restart — write, read back, replay from there', () => {
      const file = path.join(dir, 'nested', 'cursor.json');
      expect(writeCursor(file, T0 + 42)).toBe(true);
      expect(readCursor(file)).toBe(T0 + 42);
      expect(roomFilter('team', readCursor(file)).since).toBe(T0 + 42 - OVERLAP_SECS);
    });

    it('a corrupt or missing cursor degrades to cold start, never throws', () => {
      const missing = path.join(dir, 'nope.json');
      expect(readCursor(missing)).toBeNull();
      expect(roomFilter('team', readCursor(missing)).limit).toBe(COLD_START_LIMIT);

      const corrupt = path.join(dir, 'corrupt.json');
      fs.writeFileSync(corrupt, '{not json');
      expect(readCursor(corrupt)).toBeNull();

      const wrongType = path.join(dir, 'wrong.json');
      fs.writeFileSync(wrongType, JSON.stringify({ cursor: 'yesterday' }));
      expect(readCursor(wrongType)).toBeNull();
    });

    it('an unwritable cursor path is reported, not thrown — the room outlives the disk', () => {
      const blocked = path.join(dir, 'file-not-dir');
      fs.writeFileSync(blocked, 'x');
      expect(writeCursor(path.join(blocked, 'cursor.json'), T0)).toBe(false);
    });
  });
});
