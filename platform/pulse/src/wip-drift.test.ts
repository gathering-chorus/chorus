// @test-type: unit — signal is fixture-data: pure detector, no io, no timers
/**
 * #3879 — WIP-drift detector (pure core).
 *
 * Jeff, ≥6 times since 2026-04-24, latest "/fuc (finish ur card)": a role
 * active anywhere while its WIP card sits untouched. The system nudges the
 * ROLE; only a second consecutive window escalates to Jeff. DEC-1571 said
 * this in prose — prose produced the repeats; this goes red.
 */
import { detectWipDrift, DRIFT_WINDOW_MS, DriftInput } from './wip-drift';

const T0 = Date.parse('2026-08-14T12:00:00.000-04:00');
const H = 60 * 60 * 1000;

const base = (over: Partial<DriftInput> = {}): DriftInput => ({
  role: 'silas',
  cardId: 3879,
  lastRoleActivityMs: T0 + 5 * H,     // active NOW
  lastCardActivityMs: T0,             // card untouched since T0
  nowMs: T0 + 5 * H,
  priorDriftAtMs: null,
  ...over,
});

describe('#3879 detectWipDrift', () => {
  // NEGATIVE PROOF (#3734): active owner + untouched WIP card → drift fires.
  it('fires when the owner is active 4+ hours with zero card activity', () => {
    const d = detectWipDrift(base());
    expect(d).not.toBeNull();
    expect(d!.escalateToJeff).toBe(false); // first window → the ROLE, not Jeff
  });

  it('stays silent when the card was touched inside the window', () => {
    expect(detectWipDrift(base({ lastCardActivityMs: T0 + 4.5 * H }))).toBeNull();
  });

  it('stays silent when the ROLE itself is idle (idle is declared, not drift)', () => {
    expect(detectWipDrift(base({ lastRoleActivityMs: T0 + 1 * H, nowMs: T0 + 5 * H }))).toBeNull();
  });

  it('a second consecutive drift window escalates to Jeff', () => {
    const d = detectWipDrift(base({ priorDriftAtMs: T0 + 4 * H, nowMs: T0 + 9 * H, lastRoleActivityMs: T0 + 9 * H }));
    expect(d).not.toBeNull();
    expect(d!.escalateToJeff).toBe(true);
  });

  it('a card touch after a prior drift resets the escalation ladder', () => {
    const d = detectWipDrift(base({
      priorDriftAtMs: T0 + 4 * H,
      lastCardActivityMs: T0 + 5 * H,       // touched after the first drift
      nowMs: T0 + 6 * H,
      lastRoleActivityMs: T0 + 6 * H,
    }));
    expect(d).toBeNull();
  });

  it('window is 4 hours', () => {
    expect(DRIFT_WINDOW_MS).toBe(4 * H);
  });
});
