// @test-type: unit — pure projection over fixture spine events; no fs, no live state.
/**
 * #3882 — role tiles tell the truth: state is a PROJECTION of the spine
 * (Jeff: "if messages and streams are right then role state largely follows";
 * Silas's architecture ruling: last reply/werk.phase/card event per role,
 * declared role-state as override/fallback only). One clock: every age is
 * computed from event timestamps against a single `now` argument.
 */
import { projectRoleState, latestSpineActivity } from '../src/tiles-spine';

const T0 = Date.parse('2026-08-15T14:00:00Z');
const iso = (secsAgo: number) => new Date(T0 - secsAgo * 1000).toISOString();

describe('#3882 spine projection', () => {
  test('the Jeff screenshot lie: werk.phase seconds ago beats stale declared idle', () => {
    // Silas #3881 "idle 27m" while landing that minute (his 16:59 seed)
    const s = projectRoleState({
      declared: 'idle',
      lastEventAgeSecs: 40,
      lastEventKind: 'werk.phase',
      now: T0,
    });
    expect(s.state).toBe('building');
    expect(s.ageSecs).toBe(40);
  });

  test('reply.emitted counts as activity', () => {
    const s = projectRoleState({
      declared: 'idle', lastEventAgeSecs: 60, lastEventKind: 'reply.emitted', now: T0,
    });
    expect(s.state).toBe('building');
  });

  test('declared blocked survives while active (a blocked role investigates)', () => {
    const s = projectRoleState({
      declared: 'blocked', lastEventAgeSecs: 30, lastEventKind: 'observer.digest', now: T0,
    });
    expect(s.state).toBe('blocked');
  });

  test('stale everything decays to idle with the SPINE age, not the declared ts', () => {
    const s = projectRoleState({
      declared: 'building', lastEventAgeSecs: 3600, lastEventKind: 'observer.digest', now: T0,
    });
    expect(s.state).toBe('idle');
    expect(s.ageSecs).toBe(3600);
  });

  test('no events at all → unknown, never a fabricated age', () => {
    const s = projectRoleState({ declared: 'idle', lastEventAgeSecs: null, lastEventKind: null, now: T0 });
    expect(s.state).toBe('unknown');
    expect(s.ageSecs).toBeNull();
  });
});

describe('#3882 latestSpineActivity — one clock over raw lines', () => {
  const lines = [
    JSON.stringify({ timestamp: iso(500), role: 'kade', event: 'observer.digest' }),
    JSON.stringify({ timestamp: iso(90), role: 'kade', event: 'werk.phase' }),
    JSON.stringify({ timestamp: iso(30), role: 'wren', event: 'reply.emitted' }),
    JSON.stringify({ timestamp: iso(10), role: 'pulse', event: 'nudge.surfaced' }),   // machinery: ignored
    JSON.stringify({ timestamp: iso(5), role: 'system', event: 'search.query.executed' }), // ignored
    'not json at all',
  ];
  test('per-role latest activity from role-attributed events only', () => {
    const acts = latestSpineActivity(lines, T0);
    expect(acts.kade).toEqual({ ageSecs: 90, kind: 'werk.phase' });
    expect(acts.wren).toEqual({ ageSecs: 30, kind: 'reply.emitted' });
    expect(acts.silas).toBeUndefined();
  });
  test('negative proof (#3734): machinery roles never register as activity', () => {
    const acts = latestSpineActivity(lines, T0);
    expect(acts.pulse).toBeUndefined();
    expect(acts.system).toBeUndefined();
  });
});
