// @test-type: unit — signal is fixture-data: pure detector, no io, no timers
/**
 * #3879 — WIP-drift detector (pure core).
 *
 * Jeff, ≥6 times since 2026-04-24, latest "/fuc (finish ur card)": a role
 * active anywhere while its WIP card sits untouched. The system nudges the
 * ROLE; only a second consecutive window escalates to Jeff. DEC-1571 said
 * this in prose — prose produced the repeats; this goes red.
 */
import { detectWipDrift, DRIFT_WINDOW_MS, DriftInput, foldCardActivity, awaitingGoSince } from './wip-drift';

const T0 = Date.parse('2026-08-14T12:00:00.000-04:00');
const H = 60 * 60 * 1000;

const base = (over: Partial<DriftInput> = {}): DriftInput => ({
  role: 'silas',
  cardId: 3879,
  lastRoleActivityMs: T0 + 5 * H,     // active NOW
  lastCardActivityMs: T0,             // card untouched since T0
  nowMs: T0 + 5 * H,
  priorDriftAtMs: null,
  observedSinceMs: T0,                // watched the whole span
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

  // #3880 fix — the first live firing (on its own author, "496319h"): a card
  // never OBSERVED must not read as idle-since-epoch, and nothing fires until
  // a full window has actually been WATCHED.
  it('NEGATIVE PROOF: never-observed card with a young watcher does NOT fire', () => {
    expect(detectWipDrift(base({ lastCardActivityMs: 0, observedSinceMs: T0 + 4.5 * H }))).toBeNull();
  });

  it('never-observed card DOES fire once a full window has been watched, with idle capped at the observed span', () => {
    const d = detectWipDrift(base({ lastCardActivityMs: 0, observedSinceMs: T0, nowMs: T0 + 5 * H }));
    expect(d).not.toBeNull();
    expect(d!.idleCardMs).toBe(5 * H); // observed span, not since-epoch
  });
});

// ── #3936 — a card waiting on Jeff's go is not drifting ──────────────────────
//
// 2026-08-19: this watcher told Kade to "finish it, hand it off, or unpull it"
// about #3424, which was finished — presented, green, waiting four hours for a
// go. Waiting on the human is the one state where the ROLE is not the blocker,
// and it is exactly the state the old check read as drift. A watcher that
// cannot tell "stalled" from "blocked on Jeff" trains everyone to ignore it.
describe('#3936 presented-awaiting-go', () => {
  const presented = (over: Partial<DriftInput> = {}): DriftInput => ({
    ...base({ lastCardActivityMs: T0, nowMs: T0 + 5 * H, lastRoleActivityMs: T0 + 5 * H }),
    awaitingGoSinceMs: T0,
    ...over,
  });

  it('stays silent while the card is presented and unanswered', () => {
    expect(detectWipDrift(presented())).toBeNull();
  });

  it('still fires once the go landed and work then stopped', () => {
    // NEGATIVE PROOF: the exemption must be narrow. Go answered → the role owns
    // the card again, so a stall after that is drift like any other.
    const d = detectWipDrift(presented({ awaitingGoSinceMs: null }));
    expect(d).not.toBeNull();
    expect(d!.cardId).toBe(3879);
  });

  it('still fires on a card that was never presented at all', () => {
    // NEGATIVE PROOF: the ordinary stall — the case #3879 exists for — is
    // untouched by this change.
    expect(detectWipDrift(base())).not.toBeNull();
  });
});

// #3936 — the fold that answers "was this presented, and did a go answer it?"
describe('#3936 foldCardActivity', () => {
  const ev = (event: string, at: number, over: Record<string, unknown> = {}) => ({
    timestamp: new Date(at).toISOString(), role: 'kade', event, card: 3424, ...over,
  });

  it('separates presentation from the go that answers it', () => {
    const a = foldCardActivity([ev('demo.presented', T0), ev('merge.approved', T0 + 1 * H)], 'kade', 3424);
    expect(a.lastPresentedMs).toBe(T0);
    expect(a.lastGoMs).toBe(T0 + 1 * H);
    expect(awaitingGoSince(a.lastPresentedMs!, a.lastGoMs!)).toBeNull();
  });

  it('leaves an unanswered presentation awaiting', () => {
    const a = foldCardActivity([ev('demo.presented', T0)], 'kade', 3424);
    expect(awaitingGoSince(a.lastPresentedMs!, a.lastGoMs!)).toBe(T0);
  });

  it('ignores other cards and unparseable timestamps', () => {
    // NEGATIVE PROOF: another card's demo must not silence THIS card.
    const a = foldCardActivity(
      [ev('demo.presented', T0, { card: 9999 }), { timestamp: 'not-a-date', event: 'demo.presented', card: 3424 }],
      'kade', 3424,
    );
    expect(a.lastPresentedMs).toBe(0);
    expect(awaitingGoSince(a.lastPresentedMs!, a.lastGoMs!)).toBeNull();
  });
});
