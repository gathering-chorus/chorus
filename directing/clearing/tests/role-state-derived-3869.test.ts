// @test-type: unit — pure function over tile inputs, no io.
/**
 * #3869 — a role that is working must never render "idle".
 *
 * Every screenshot Jeff sent across two days showed the same lie:
 *
 *   2026-08-13 17:15   Wren  idle 19m   (mid-build, #3862)
 *   2026-08-13 18:19   Wren  idle 11m   (mid-build, #3865)
 *   2026-08-14 08:21   Wren  idle 1h    (worked the whole hour)
 *
 * The card reads `<role>-declared.json`, which an agent sets by hand at
 * transitions and forgets. So it reports the last time somebody remembered,
 * not what is happening. Meanwhile `<role>-observations.jsonl` has been
 * carrying real per-command activity with timestamps the whole time — the tile
 * already reads it for `lastActionAge` and then ignores it for `state`.
 *
 * NEGATIVE PROOF (#3734): `fresh activity beats a stale declared idle` is the
 * case that fails against today's code. The inverse case is here too, because
 * a rule that only ever says "active" is exactly as useless as one that only
 * ever says "idle" — it cannot answer the question the card exists for.
 */

import { deriveState, ACTIVE_WINDOW_SECS } from '../src/tiles';

describe('#3869 — state is derived, not remembered', () => {
  it('fresh activity beats a stale declared idle', () => {
    expect(deriveState({ declared: 'idle', lastActivityAgeSecs: 30 })).toBe('building');
  });

  it('activity just inside the window still counts', () => {
    expect(
      deriveState({ declared: 'idle', lastActivityAgeSecs: ACTIVE_WINDOW_SECS - 1 }),
    ).toBe('building');
  });

  it('no recent activity renders idle — the rule can still say idle', () => {
    expect(deriveState({ declared: 'building', lastActivityAgeSecs: 600 })).toBe('idle');
  });

  it('never-seen activity renders idle', () => {
    expect(deriveState({ declared: 'idle', lastActivityAgeSecs: null })).toBe('idle');
  });

  // A role saying it is stuck is information inference cannot produce. Letting
  // activity overwrite it would hide the one state Jeff most needs to see —
  // and a blocked role often keeps emitting commands while it investigates.
  it('an explicit blocked declaration survives fresh activity', () => {
    expect(deriveState({ declared: 'blocked', lastActivityAgeSecs: 5 })).toBe('blocked');
  });

  it('an explicit observing declaration survives fresh activity', () => {
    expect(deriveState({ declared: 'observing', lastActivityAgeSecs: 5 })).toBe('observing');
  });

  it('but a declared block does NOT survive going quiet — stale is stale', () => {
    expect(deriveState({ declared: 'blocked', lastActivityAgeSecs: 3600 })).toBe('idle');
  });
});
