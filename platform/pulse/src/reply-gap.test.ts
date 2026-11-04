// @test-type: unit — signal is fixture-data: pure detector, no io, no timers
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
// @card: #3864
// @owner: wren
/**
 * #3864 — reply-delivery gap detector (pure core).
 *
 * reply.emitted (Stop hook) with no row in the room's record inside the
 * window = a delivery gap Jeff currently discovers by asking "did you see
 * that?". #4887: the room's own rows decide delivery, not a spine echo.
 * The detector is pure: events + rows in, gaps out.
 */
import { detectGaps, gapKey, SpineEv, RoomMsg } from './reply-gap';
import { contentHash } from '../../chorus-sdk/reply/hash';

const T0 = Date.parse('2026-08-13T20:00:00.000-04:00');
const ev = (event: string, role: string, hash: string, atMs: number): SpineEv => ({
  event, role, hash, timestamp: new Date(atMs).toISOString(),
});

const WINDOW = 60_000;
// The room's record reaches back before T0, so a missing row is measurable.
const room = (...texts: string[]): RoomMsg[] => [
  { from: 'wren', text: 'an older row', ts: new Date(T0 - 3_600_000).toISOString() },
  ...texts.map((text) => ({ from: 'silas', text, ts: new Date(T0 + 2000).toISOString() })),
];
const gapsOf = (events: SpineEv[], r: RoomMsg[] | null, now: number, fired = new Set<string>()) =>
  detectGaps(events, r, now, WINDOW, fired).gaps;

describe('#3864 detectGaps', () => {
  // NEGATIVE PROOF (#3734, the card's AC3): an emitted with NO rendered must
  // fire once the window elapses — this is the state the detector exists to catch.
  it('fires a gap for an unrendered reply after the window', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    const gaps = gapsOf(events, room(), T0 + WINDOW + 1);
    expect(gaps).toHaveLength(1);
    expect(gaps[0]).toMatchObject({ role: 'silas', hash: 'abc123' });
  });

  it('stays quiet while the window is still open', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    expect(gapsOf(events, room(), T0 + WINDOW - 1000)).toHaveLength(0);
  });

  it('a room row with the same hash closes the pair — no gap', () => {
    const events = [ev('reply.emitted', 'silas', contentHash('hello Jeff'), T0)];
    expect(gapsOf(events, room('hello Jeff'), T0 + WINDOW + 1)).toHaveLength(0);
  });

  it('a room row for a DIFFERENT reply does not mask the gap', () => {
    const events = [ev('reply.emitted', 'silas', contentHash('hello Jeff'), T0)];
    expect(gapsOf(events, room('something else'), T0 + WINDOW + 1)).toHaveLength(1);
  });

  it('already-fired gaps do not re-fire on the next scan', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    const fired = new Set<string>();
    const first = gapsOf(events, room(), T0 + WINDOW + 1, fired);
    expect(first).toHaveLength(1);
    fired.add(gapKey(first[0]));
    expect(gapsOf(events, room(), T0 + WINDOW + 5000, fired)).toHaveLength(0);
  });

  it('ignores malformed events (no hash / no timestamp) rather than throwing', () => {
    const events: SpineEv[] = [
      { event: 'reply.emitted', role: 'silas' },
      { event: 'reply.emitted', hash: 'x', role: 'silas', timestamp: 'not-a-date' },
    ];
    expect(gapsOf(events, room(), T0)).toHaveLength(0);
  });
});
