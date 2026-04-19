// withDb helper (extracted from server.ts for #2205 wave 17).
//
// Routes that need a readonly db all repeat the same 8-line boilerplate:
// try to open, catch DbNotFoundError → 503, otherwise rethrow, run work,
// close in a finally. This helper encapsulates that so each route body is
// just the work.

import { DbNotFoundError } from './server-helpers';

interface MinimalResponse {
  status: (s: number) => MinimalResponse;
  json: (b: any) => MinimalResponse | void;
}

interface MinimalDb {
  close: () => void;
}

export function createWithDb<DB extends MinimalDb = any>(
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
    } finally {
      db.close();
    }
  };
}
