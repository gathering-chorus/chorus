/**
 * #3086 — main-side dispatcher for the FTS worker thread. Thin wrapper over the
 * generic worker pool (#3382): FTS is one backend of createWorkerPool, sharing
 * the same lazy-spawn / id-correlation / timeout / death-respawn logic as the
 * semantic-search worker. The only FTS-specific bits live here — the FtsRequest
 * shape and the 'fts' label. The dispatcher guarantees every request settles
 * (reply, timeout, or crash); no request can hang silently (AC: "worker errors
 * surface as proper API errors, not silent hangs").
 *
 * `spawn` is injected so the pool is unit-testable with a fake worker — no real
 * thread, no flakiness. WorkerLike re-exported for callers that build the spawn.
 */
import type { FtsRequest } from './fts-worker-core';
import { createWorkerPool, type WorkerLike } from './worker-pool';

export type { WorkerLike };

export interface FtsQuery {
  q: string;
  fetchLimit: number;
  role?: string;
  mode: string;
}

export interface FtsPoolOptions {
  spawn: () => WorkerLike;
  timeoutMs?: number;
}

export interface FtsPool {
  runFtsAsync(query: FtsQuery): Promise<unknown[]>;
  shutdown(): void;
}

export function createFtsPool(opts: FtsPoolOptions): FtsPool {
  const pool = createWorkerPool<FtsQuery, FtsRequest>({
    spawn: opts.spawn,
    timeoutMs: opts.timeoutMs,
    label: 'fts',
    buildRequest: (id, q) => ({ id, q: q.q, fetchLimit: q.fetchLimit, role: q.role, mode: q.mode }),
  });
  return {
    runFtsAsync: (query) => pool.run(query),
    shutdown: () => pool.shutdown(),
  };
}
