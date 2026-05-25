/**
 * #3086 — main-side dispatcher for the FTS worker thread. Holds one persistent
 * worker (lazy-spawned), correlates replies by id, and guarantees every request
 * settles: a reply resolves/rejects it, a timeout rejects it, and a worker crash
 * rejects all in-flight requests + drops the worker for respawn. No request can
 * hang silently (AC: "worker errors surface as proper API errors, not silent hangs").
 *
 * `spawn` is injected so the pool logic is unit-testable with a fake worker —
 * no real thread, no flakiness.
 */
import type { FtsRequest, FtsReply } from './fts-worker-core';

export interface WorkerLike {
  postMessage(msg: unknown): void;
  on(event: 'message' | 'error' | 'exit', cb: (arg: unknown) => void): void;
  terminate?(): unknown;
}

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

interface Pending {
  resolve: (rows: unknown[]) => void;
  reject: (e: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export function createFtsPool(opts: FtsPoolOptions): FtsPool {
  const timeoutMs = opts.timeoutMs ?? 10_000;
  const pending = new Map<number, Pending>();
  let worker: WorkerLike | null = null;
  let nextId = 1;
  let shuttingDown = false;

  function rejectAll(err: Error): void {
    for (const [, p] of pending) {
      clearTimeout(p.timer);
      p.reject(err);
    }
    pending.clear();
  }

  function ensureWorker(): WorkerLike {
    if (worker) return worker;
    const w = opts.spawn();
    w.on('message', (reply) => {
      const r = reply as FtsReply;
      const p = pending.get(r.id);
      if (!p) return;
      clearTimeout(p.timer);
      pending.delete(r.id);
      if ('error' in r) p.reject(new Error(r.error));
      else p.resolve(r.rows);
    });
    w.on('error', (err) => {
      worker = null; // drop so the next request respawns
      rejectAll(err instanceof Error ? err : new Error(String(err)));
    });
    w.on('exit', (code) => {
      worker = null;
      if (!shuttingDown && code !== 0) rejectAll(new Error(`fts worker exited with code ${String(code)}`));
    });
    worker = w;
    return w;
  }

  return {
    runFtsAsync(query) {
      return new Promise<unknown[]>((resolve, reject) => {
        const id = nextId++;
        const w = ensureWorker();
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`fts worker timeout after ${timeoutMs}ms`));
        }, timeoutMs);
        // Don't keep the process alive on the timer alone.
        const t = timer as unknown as { unref?: () => void };
        if (typeof t.unref === 'function') t.unref();
        pending.set(id, { resolve, reject, timer });
        const msg: FtsRequest = { id, q: query.q, fetchLimit: query.fetchLimit, role: query.role, mode: query.mode };
        w.postMessage(msg);
      });
    },
    shutdown() {
      shuttingDown = true;
      rejectAll(new Error('fts pool shut down'));
      if (worker && worker.terminate) worker.terminate();
      worker = null;
    },
  };
}
