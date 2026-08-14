// @test-type: unit — signal is fixture-data: pure detector, no io, no timers
/**
 * #3864 — reply-delivery gap detector (pure core).
 *
 * reply.emitted (Stop hook) without reply.rendered (Clearing) inside the
 * window = a delivery gap Jeff currently discovers by asking "did you see
 * that?". The detector is pure: events in, gaps out; the service leg only
 * feeds it the spine tail and emits what it returns.
 */
import { detectGaps, gapKey, SpineEv } from './reply-gap';

const T0 = Date.parse('2026-08-13T20:00:00.000-04:00');
const ev = (event: string, role: string, hash: string, atMs: number): SpineEv => ({
  event, role, hash, timestamp: new Date(atMs).toISOString(),
});

const WINDOW = 60_000;

describe('#3864 detectGaps', () => {
  // NEGATIVE PROOF (#3734, the card's AC3): an emitted with NO rendered must
  // fire once the window elapses — this is the state the detector exists to catch.
  it('fires a gap for an unrendered reply after the window', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    const gaps = detectGaps(events, T0 + WINDOW + 1, WINDOW, new Set());
    expect(gaps).toHaveLength(1);
    expect(gaps[0]).toMatchObject({ role: 'silas', hash: 'abc123' });
  });

  it('stays quiet while the window is still open', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    expect(detectGaps(events, T0 + WINDOW - 1000, WINDOW, new Set())).toHaveLength(0);
  });

  it('a rendered with the matching role+hash closes the pair — no gap', () => {
    const events = [
      ev('reply.emitted', 'silas', 'abc123', T0),
      ev('reply.rendered', 'silas', 'abc123', T0 + 2000),
    ];
    expect(detectGaps(events, T0 + WINDOW + 1, WINDOW, new Set())).toHaveLength(0);
  });

  it('a rendered for a DIFFERENT reply does not mask the gap', () => {
    const events = [
      ev('reply.emitted', 'silas', 'abc123', T0),
      ev('reply.rendered', 'silas', 'zzz999', T0 + 2000),
    ];
    expect(detectGaps(events, T0 + WINDOW + 1, WINDOW, new Set())).toHaveLength(1);
  });

  it('already-fired gaps do not re-fire on the next scan', () => {
    const events = [ev('reply.emitted', 'silas', 'abc123', T0)];
    const fired = new Set<string>();
    const first = detectGaps(events, T0 + WINDOW + 1, WINDOW, fired);
    expect(first).toHaveLength(1);
    fired.add(gapKey(first[0]));
    expect(detectGaps(events, T0 + WINDOW + 5000, WINDOW, fired)).toHaveLength(0);
  });

  it('ignores malformed events (no hash / no timestamp) rather than throwing', () => {
    const events: SpineEv[] = [
      { event: 'reply.emitted', role: 'silas' },
      { event: 'reply.emitted', hash: 'x', role: 'silas', timestamp: 'not-a-date' },
    ];
    expect(detectGaps(events, T0, WINDOW, new Set())).toHaveLength(0);
  });
});
