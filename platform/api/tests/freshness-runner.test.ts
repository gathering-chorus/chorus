// @test-type: unit — real worker_threads in this process, worker bodies given inline;
// no service, no store, no index.
// #3060 (reopened 2026-10-07) — the freshness COUNT took 11-15s on chorus-api's
// main thread and froze every request behind it, the Clearing included.
// These tests run a REAL worker thread whose body is busy for 1.5s, the way the
// COUNT is, and measure how late the main thread's timers fire meanwhile.
//
// AC1: through the runner, the main thread keeps ticking (worst lag < 100ms).
// AC2 (negative proof): the same busy work run in-process (the old path) delays
// the timers by the full 1.5s, so this measurement can tell the two states apart.

import { Worker } from 'node:worker_threads';
import { createFreshnessRunner } from '../src/freshness-runner';
import { createFreshnessCache } from '../src/freshness-cache';

const BUSY_MS = 1500;

// The worker body: block its own thread for BUSY_MS, then reply like freshness-worker.ts.
const slowWorkerSource = `
  const { parentPort } = require('node:worker_threads');
  parentPort.on('message', (msg) => {
    const end = Date.now() + ${BUSY_MS};
    while (Date.now() < end) { /* the slow COUNT */ }
    parentPort.postMessage({ id: msg.id, rows: [{ status: 200, body: { counted: true } }] });
  });
`;

function busyOnThisThread(): { status: number; body: unknown } {
  const end = Date.now() + BUSY_MS;
  while (Date.now() < end) { /* the slow COUNT, on the main thread */ }
  return { status: 200, body: { counted: true } };
}

// Run `work` while a 20ms timer ticks; return the result and the worst lag seen.
async function worstTimerLag<T>(work: () => Promise<T>): Promise<{ result: T; worstLagMs: number }> {
  let worst = 0;
  let last = Date.now();
  const tick = setInterval(() => {
    const now = Date.now();
    worst = Math.max(worst, now - last - 20);
    last = now;
  }, 20);
  try {
    await new Promise((r) => setTimeout(r, 40)); // let the timer start ticking first
    const result = await work();
    await new Promise((r) => setTimeout(r, 30)); // let a final tick land after a blocking run
    return { result, worstLagMs: worst };
  } finally {
    clearInterval(tick);
  }
}

describe('freshness runs in a worker thread (#3060 reopen)', () => {
  jest.setTimeout(20_000);

  it('AC1: a 1.5s recompute in the worker leaves the main thread answering (< 100ms lag)', async () => {
    const runner = createFreshnessRunner(() => new Worker(slowWorkerSource, { eval: true }));
    const cache = createFreshnessCache(runner.run, { ttlMs: 30_000 });
    const { result, worstLagMs } = await worstTimerLag(() => cache.get());
    runner.shutdown();
    expect(result).toEqual({ status: 200, body: { counted: true } });
    expect(worstLagMs).toBeLessThan(100);
  });

  it('AC2 NEGATIVE PROOF: the same 1.5s recompute in-process (the old path) blocks the main thread for 1.5s', async () => {
    const cache = createFreshnessCache(async () => busyOnThisThread(), { ttlMs: 30_000 });
    const { result, worstLagMs } = await worstTimerLag(() => cache.get());
    expect(result).toEqual({ status: 200, body: { counted: true } });
    expect(worstLagMs).toBeGreaterThanOrEqual(BUSY_MS - 100);
  });

  it('a worker that reports an error surfaces as a rejected recompute, not a hang', async () => {
    const runner = createFreshnessRunner(
      () => new Worker(`
        const { parentPort } = require('node:worker_threads');
        parentPort.on('message', (m) => parentPort.postMessage({ id: m.id, error: 'index locked' }));
      `, { eval: true }),
    );
    await expect(runner.run()).rejects.toThrow('index locked');
    runner.shutdown();
  });
});
