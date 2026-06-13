/**
 * #3382 — generic request/response worker pool, extracted from the #3086 FTS
 * pool so FTS (worker_threads) and semantic search (separate process) share ONE
 * dispatcher implementation instead of two. Holds one persistent worker
 * (lazy-spawned), correlates replies by id, and guarantees every request
 * settles: a reply resolves/rejects it, a timeout rejects it, and a worker
 * crash rejects all in-flight requests + drops the worker for respawn. No
 * request can hang silently.
 *
 * `spawn` is injected (the WorkerLike seam), so the pool is transport-agnostic:
 * FTS injects a worker_threads Worker; semantic search injects a child_process
 * fork adapted to WorkerLike (the lance native CPU pool MUST be in a separate
 * OS process, #3382 — a worker_thread shares the process and would not isolate
 * it). `buildRequest` shapes the per-backend message; the reply shape is shared.
 */
export interface WorkerLike {
  postMessage(msg: unknown): void;
  on(event: 'message' | 'error' | 'exit', cb: (arg: unknown) => void): void;
  terminate?(): unknown;
}

/** Every worker reply carries the correlation id; rows on success, error on failure. */
export type WorkerReply = { id: number; rows: unknown[] } | { id: number; error: string };

export interface WorkerPoolOptions<Query, Req extends { id: number }> {
  spawn: () => WorkerLike;
  /** Shape the wire message for this backend from the public query + a fresh id. */
  buildRequest: (id: number, query: Query) => Req;
  timeoutMs?: number;
  /** Used in timeout/exit error messages so failures name their backend. */
  label?: string;
}

export interface WorkerPool<Query> {
  run(query: Query): Promise<unknown[]>;
  shutdown(): void;
}

interface Pending {
  resolve: (rows: unknown[]) => void;
  reject: (e: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export function createWorkerPool<Query, Req extends { id: number }>(
  opts: WorkerPoolOptions<Query, Req>,
): WorkerPool<Query> {
  const timeoutMs = opts.timeoutMs ?? 10_000;
  const label = opts.label ?? 'worker';
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
      const r = reply as WorkerReply;
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
      if (!shuttingDown && code !== 0) rejectAll(new Error(`${label} worker exited with code ${String(code)}`));
    });
    worker = w;
    return w;
  }

  return {
    run(query) {
      return new Promise<unknown[]>((resolve, reject) => {
        const id = nextId++;
        const w = ensureWorker();
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`${label} worker timeout after ${timeoutMs}ms`));
        }, timeoutMs);
        // Don't keep the process alive on the timer alone.
        const t = timer as unknown as { unref?: () => void };
        if (typeof t.unref === 'function') t.unref();
        pending.set(id, { resolve, reject, timer });
        w.postMessage(opts.buildRequest(id, query));
      });
    },
    shutdown() {
      shuttingDown = true;
      rejectAll(new Error(`${label} pool shut down`));
      if (worker && worker.terminate) worker.terminate();
      worker = null;
    },
  };
}
