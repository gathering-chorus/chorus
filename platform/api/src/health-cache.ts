import type { MessageCounts } from './message-counts';

// Deep-health cache (extracted from server.ts for #2205 wave 13).
//
// Cached so /api/chorus/health/detail doesn't shell / count on every
// request. Refreshed every 30s by a server.ts interval. Factory form
// lets tests hit every branch without a real sqlite, lance table, or
// filesystem.

export interface HealthSnapshot {
  dbRows: number;
  unembedded: number;
  vectors: number;
  dbStatus: string;
  hooksStatus: string;
  ts: number;
}

export interface HealthCacheDeps {
  /** #4556 — the two message counts, answered off the event loop (server.ts runs them
   *  in a worker thread). A sync sqlite read here held the loop 3.97 s on 2026-10-16. */
  countMessages: () => Promise<MessageCounts>;
  getLanceTable: () => { countRows: () => Promise<number> } | null;
  fs: {
    existsSync: (p: string) => boolean;
    statSync: (p: string) => { mtimeMs: number };
  };
  hookBinaryPath: string;
  now?: () => number;
}

export interface HealthCache {
  refresh: () => Promise<void>;
  snapshot: () => HealthSnapshot;
}

export function createHealthCache(deps: HealthCacheDeps): HealthCache {
  const now = deps.now ?? (() => Date.now());
  const state: HealthSnapshot = {
    dbRows: 0,
    unembedded: 0,
    vectors: 0,
    dbStatus: 'unknown',
    hooksStatus: 'unknown',
    ts: 0,
  };

  return {
    snapshot: () => state,
    async refresh() {
      try {
        const counts = await deps.countMessages();
        state.dbRows = counts.dbRows;
        if (counts.unembedded !== null) state.unembedded = counts.unembedded;
        state.dbStatus = 'ok';
      } catch {
        state.dbStatus = 'error';
      }
      try {
        const table = deps.getLanceTable();
        if (table) state.vectors = await table.countRows();
      } catch {
        /* non-fatal */
      }
      try {
        if (deps.fs.existsSync(deps.hookBinaryPath)) {
          const stat = deps.fs.statSync(deps.hookBinaryPath);
          state.hooksStatus = (now() - stat.mtimeMs) / 3600000 < 24 ? 'active' : 'stale';
        } else {
          state.hooksStatus = 'missing';
        }
      } catch {
        state.hooksStatus = 'error';
      }
      state.ts = now();
    },
  };
}
