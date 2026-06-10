// #3082 — external-probe eventloop detector tests (jest / ts-jest, the api package's runner).
//
// The bug: the in-process `blocked` detector (eventloop-alert.ts) rides the very loop
// it measures, so a hard block starves its own timer and it under-reports. The fix:
// an OUT-OF-PROCESS worker probes chorus-api's response latency on an interval — its
// own loop is idle, so it measures the block accurately from outside.
//
// Pins the PURE evaluation core (latency → BlockAlert | null) and the worker's
// throttled emit/nudge wiring. No real HTTP, no real timers.

import { evaluateProbe, runEventloopProbe } from '../src/eventloop-probe';

describe('evaluateProbe (pure: a measured probe latency over threshold IS a block)', () => {
  it('latency at/over threshold → BlockAlert carrying the measured duration', () => {
    const a = evaluateProbe({ latencyMs: 4200, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe' });
    expect(a).toBeTruthy();
    expect(a!.duration_ms).toBe(4200);
    expect(a!.message).toMatch(/event loop blocked 4200ms/);
  });

  it('a timed-out probe reports the timeout as the block duration', () => {
    const a = evaluateProbe({ latencyMs: 5000, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe', timedOut: true });
    expect(a).toBeTruthy();
    expect(a!.message).toMatch(/5000ms/);
    expect(a!.op).toMatch(/timeout|probe/);
  });

  it('under-threshold latency is NOT a block (healthy loop answers fast)', () => {
    expect(evaluateProbe({ latencyMs: 120, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe' })).toBeNull();
  });
});

describe('runEventloopProbe (worker: one alert per block EPISODE, measure from outside)', () => {
  it('emits once per block episode, re-arms on recovery; nudges throttled', async () => {
    const emits: number[] = [];
    const nudges: number[] = [];
    let clock = 0;
    // block, still-blocked, RECOVER, block again → two distinct episodes
    const latencies = [3000, 3000, 80, 3000];
    let i = 0;
    await runEventloopProbe({
      probe: async () => ({ latencyMs: latencies[i++] ?? 80, timedOut: false }),
      emit: (a) => emits.push(a.duration_ms),
      nudge: (a) => nudges.push(a.duration_ms),
      threshold: 1000,
      throttleMs: 300_000,
      now: () => clock,
      sleep: async () => { clock += 100_000; },
      ticks: 4,
    });
    // tick0 episode-start (emit), tick1 still-blocked (suppress), tick2 recover (re-arm),
    // tick3 NEW episode (emit). One emit per episode — no per-tick flood.
    expect(emits).toEqual([3000, 3000]);
    expect(nudges).toEqual([3000, 3000]); // throttled: tick0 (clock 0) + tick3 (clock 300000)
  });

  it('a SUSTAINED block (every probe slow, no recovery) emits ONCE, not per probe — no flood', async () => {
    // #3082 Kade review: the synthetic-flood lesson — a chronically-slow loop must not
    // emit an eventloop.blocked event on every 2s probe.
    const emits: number[] = [];
    await runEventloopProbe({
      probe: async () => ({ latencyMs: 5000, timedOut: false }),
      emit: () => emits.push(1),
      nudge: () => {},
      threshold: 1000,
      now: () => 0,
      sleep: async () => {},
      ticks: 10,
    });
    expect(emits).toHaveLength(1); // 10 slow probes, one unbroken episode → one emit
  });

  it('a healthy loop (fast probes) emits nothing — no false blocks', async () => {
    const emits: number[] = [];
    await runEventloopProbe({
      probe: async () => ({ latencyMs: 80, timedOut: false }),
      emit: (a) => emits.push(a.duration_ms),
      nudge: () => {},
      threshold: 1000,
      now: () => 0,
      sleep: async () => {},
      ticks: 5,
    });
    expect(emits).toHaveLength(0);
  });
});
