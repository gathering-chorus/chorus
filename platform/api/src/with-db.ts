// withDb helper (extracted from server.ts for #2205 wave 17).
//
// Routes that need a readonly db all repeat the same 8-line boilerplate:
// try to open, catch DbNotFoundError → 503, otherwise rethrow, run work,
// close in a finally. This helper encapsulates that so each route body is
// just the work.

import { DbNotFoundError } from './server-helpers';

interface MinimalResponse {
  status: (s: number) => MinimalResponse;
  json: (b: unknown) => MinimalResponse | void;
  headersSent?: boolean;
}

interface MinimalDb {
  close: () => void;
}

export function createWithDb<DB extends MinimalDb = MinimalDb>(
  openDb: () => DB,
): <T>(res: MinimalResponse, work: (db: DB) => Promise<T> | T) => Promise<T | undefined> {
  return async function withDb<T>(
    res: MinimalResponse,
    work: (db: DB) => Promise<T> | T,
  ): Promise<T | undefined> {
    let db: DB;
    try {
      db = openDb();
    } catch (e) {
      if (e instanceof DbNotFoundError) {
        res.status(503).json!({ error: e.message });
        return undefined;
      }
      throw e;
    }
    try {
      return await work(db);
    } catch (e) {
      // #3750 — a route failure is a PER-REQUEST failure, never a process
      // death. Pre-fix, a rejection here propagated through Express 4's async
      // handler into unhandledRejection → the FATAL handler → exit(1): on
      // 2026-08-04 10:47 one slow fts worker (timeout rejection) killed the
      // whole API. Now: typed response for this request, everything else
      // keeps serving. 503 for the pool-timeout class (transient, retryable),
      // 500 otherwise.
      const msg = e instanceof Error ? e.message : String(e);
      console.error(`[chorus-api] route work failed (degraded to per-request error): ${msg}`);
      if (!res.headersSent) {
        const status = msg.includes('worker timeout') || msg.includes('pool shut down') ? 503 : 500;
        res.status(status).json!({ error: msg, degraded: true });
      }
      return undefined;
    } finally {
      db.close();
    }
  };
}
