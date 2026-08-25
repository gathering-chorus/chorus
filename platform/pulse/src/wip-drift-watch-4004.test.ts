// @test-type: unit — fake timers + injected readers; no board, no spine, no clock
//
// #4004 — wip-drift.ts sat at 45% function coverage. The pure rule was tested;
// the WATCH LOOP was not — and the loop is where the two hard-won fixes live:
// #3880 (its first live firing was a false positive on its own author, fixed by
// accumulating newest-seen across ticks and dating observation per card) and
// #3936 (a card presented and awaiting Jeff's go is not drifting). Untested,
// either could regress into nudging Jeff about cards that are waiting on him.
import {
  startWipDriftWatch,
  DRIFT_WINDOW_MS,
  ROLE_ACTIVE_MS,
  type Drift,
} from './wip-drift';

const T0 = Date.parse('2026-08-25T12:00:00.000Z');
const OLD = T0 - DRIFT_WINDOW_MS - 60_000;   // beyond the drift window
const CARD = [{ id: 4004, owner: 'Silas' }];

describe('#4004 wip-drift watch loop', () => {
  afterEach(() => { jest.useRealTimers(); });

  it('#3880: the FIRST window is unwatched, so nothing fires on first sight', async () => {
    jest.useFakeTimers();
    const fired: Drift[] = [];
    const stop = startWipDriftWatch(
      async () => CARD,
      async () => ({ lastCardActivityMs: OLD, lastRoleActivityMs: T0 - 60_000 }),
      async (d) => { fired.push(d); },
      1000,
      () => T0,
    );
    await jest.advanceTimersByTimeAsync(1000);
    stop();
    expect(fired).toHaveLength(0);
  });

  it('an active role whose card has been idle past the OBSERVED window drifts — once', async () => {
    jest.useFakeTimers();
    const fired: Drift[] = [];
    let now = T0;
    const stop = startWipDriftWatch(
      async () => CARD,
      async () => ({ lastCardActivityMs: OLD, lastRoleActivityMs: now }),
      async (d) => { fired.push(d); },
      1000,
      () => now,
    );
    await jest.advanceTimersByTimeAsync(1000);       // observation starts here
    now = T0 + DRIFT_WINDOW_MS + 60_000;             // the observed window elapses
    await jest.advanceTimersByTimeAsync(3000);       // several more ticks
    stop();
    expect(fired).toHaveLength(1);
    expect(fired[0]).toMatchObject({ role: 'silas', cardId: 4004 });
  });

  it("#3936 NEGATIVE PROOF: a card presented and awaiting Jeff's go never drifts", async () => {
    jest.useFakeTimers();
    const fired: Drift[] = [];
    let now = T0;
    const stop = startWipDriftWatch(
      async () => CARD,
      async () => ({
        lastCardActivityMs: OLD,
        lastRoleActivityMs: now,
        lastPresentedMs: T0 - 120_000,   // presented...
        lastGoMs: 0,                     // ...and no go answered it
      }),
      async (d) => { fired.push(d); },
      1000,
      () => now,
    );
    await jest.advanceTimersByTimeAsync(1000);
    now = T0 + DRIFT_WINDOW_MS + 60_000;
    await jest.advanceTimersByTimeAsync(3000);
    stop();
    expect(fired).toHaveLength(0);
  });

  it('once the go lands the role owns the card again, and an ordinary stall drifts', async () => {
    jest.useFakeTimers();
    const fired: Drift[] = [];
    let now = T0;
    const stop = startWipDriftWatch(
      async () => CARD,
      async () => ({
        lastCardActivityMs: OLD,
        lastRoleActivityMs: now,
        lastPresentedMs: T0 - 300_000,
        lastGoMs: T0 - 120_000,          // go answered the presentation
      }),
      async (d) => { fired.push(d); },
      1000,
      () => now,
    );
    await jest.advanceTimersByTimeAsync(1000);
    now = T0 + DRIFT_WINDOW_MS + 60_000;
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(fired).toHaveLength(1);
  });

  it('a silent ROLE is not drift — nothing fires when the role itself is idle', async () => {
    jest.useFakeTimers();
    const fired: Drift[] = [];
    let now = T0;
    const stop = startWipDriftWatch(
      async () => CARD,
      async () => ({
        lastCardActivityMs: OLD,
        lastRoleActivityMs: T0 - ROLE_ACTIVE_MS - 60_000,
      }),
      async (d) => { fired.push(d); },
      1000,
      () => now,
    );
    await jest.advanceTimersByTimeAsync(1000);
    now = T0 + DRIFT_WINDOW_MS + 60_000;
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(fired).toHaveLength(0);
  });

  it('a failing read is LOUD but not fatal — the loop keeps scanning', async () => {
    jest.useFakeTimers();
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    let calls = 0;
    const stop = startWipDriftWatch(
      async () => { calls += 1; if (calls === 1) throw new Error('board unreachable'); return []; },
      async () => ({ lastCardActivityMs: 0, lastRoleActivityMs: 0 }),
      async () => {},
      1000,
      () => T0,
    );
    await jest.advanceTimersByTimeAsync(2000);
    stop();
    expect(err).toHaveBeenCalled();
    expect(calls).toBeGreaterThan(1);
    err.mockRestore();
  });

  it('stop() ends the watch', async () => {
    jest.useFakeTimers();
    let reads = 0;
    const stop = startWipDriftWatch(
      async () => { reads += 1; return []; },
      async () => ({ lastCardActivityMs: 0, lastRoleActivityMs: 0 }),
      async () => {},
      1000,
      () => T0,
    );
    await jest.advanceTimersByTimeAsync(1000);
    const afterFirst = reads;
    stop();
    await jest.advanceTimersByTimeAsync(5000);
    expect(reads).toBe(afterFirst);
  });
});
