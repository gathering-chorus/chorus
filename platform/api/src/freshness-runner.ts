/**
 * #3060 — main-side runner for the freshness worker thread. One long-lived
 * worker, reused through the shared worker pool (#3382): lazy spawn, id
 * correlation, a timeout, and respawn after a crash. The recompute's COUNT
 * runs in that thread, so chorus-api's main thread only awaits a message.
 */
import { createWorkerPool, type WorkerLike } from './worker-pool';
import type { FreshnessResult } from './freshness-compute';

export function createFreshnessRunner(
  spawn: () => WorkerLike,
  timeoutMs = 120_000,
): { run: () => Promise<FreshnessResult>; shutdown: () => void } {
  const pool = createWorkerPool<void, { id: number }>({
    spawn,
    timeoutMs,
    label: 'freshness',
    buildRequest: (id) => ({ id }),
  });
  return {
    run: async () => (await pool.run(undefined))[0] as FreshnessResult,
    shutdown: () => pool.shutdown(),
  };
}
