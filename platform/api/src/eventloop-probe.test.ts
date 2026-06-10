// #3082 — external-probe eventloop detector tests.
//
// The bug: the in-process `blocked` detector (eventloop-alert.ts) rides the very
// loop it measures, so a hard block starves the detector and it under-reports.
// The fix: an OUT-OF-PROCESS worker probes chorus-api's response latency on an
// interval. When the loop is blocked the probe is slow (or times out) — and the
// worker's own loop is idle, so it measures the block accurately from outside.
//
// These pin the PURE evaluation core (latency → BlockAlert | null) and the
// worker's throttled emit/nudge wiring. No real HTTP, no real timers.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { evaluateProbe, runEventloopProbe } from './eventloop-probe';

// --- pure core: a measured probe latency over threshold IS a block ---

test('evaluateProbe: latency at/over threshold → BlockAlert with the measured duration', () => {
  const a = evaluateProbe({ latencyMs: 4200, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe' });
  assert.ok(a, 'over-threshold latency is a block');
  assert.equal(a!.duration_ms, 4200);
  assert.match(a!.message, /event loop blocked 4200ms/);
});

test('evaluateProbe: a timed-out probe reports the timeout as the block duration', () => {
  // probe timeout = the loop never answered within the window → the block is AT LEAST the timeout.
  const a = evaluateProbe({ latencyMs: 5000, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe', timedOut: true });
  assert.ok(a, 'timeout is a block');
  assert.match(a!.message, /5000ms/);
  assert.match(a!.op, /timeout|probe/);
});

test('evaluateProbe: under-threshold latency is NOT a block (healthy loop answers fast)', () => {
  assert.equal(evaluateProbe({ latencyMs: 120, threshold: 1000, ts: '2026-06-10T20:00:00.000Z', op: 'probe' }), null);
});

// --- worker wiring: emit every block, throttle nudges, measure from OUTSIDE ---

test('runEventloopProbe: emits on every over-threshold probe, throttles nudges', async () => {
  const emits: number[] = [];
  const nudges: number[] = [];
  let clock = 0;
  // three slow probes in quick succession, then one after the throttle window
  const latencies = [3000, 3000, 3000, 3000];
  let i = 0;
  await runEventloopProbe({
    probe: async () => ({ latencyMs: latencies[i++] ?? 0, timedOut: false }),
    emit: (a) => emits.push(a.duration_ms),
    nudge: (a) => nudges.push(a.duration_ms),
    threshold: 1000,
    throttleMs: 300_000,
    now: () => clock,
    sleep: async () => { clock += 100_000; }, // each tick advances 100s
    ticks: 4,
  });
  assert.equal(emits.length, 4, 'every block emits a spine record (cheap, real data)');
  // nudges throttled: tick0 fires (clock 0), next at clock>=300000 — tick3 (clock 300000)
  assert.deepEqual(nudges, [3000, 3000], 'nudges throttled to the 5-min window, not one per block');
});

test('runEventloopProbe: a healthy loop (fast probes) emits nothing', async () => {
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
  assert.equal(emits.length, 0, 'fast loop → no false blocks');
});
