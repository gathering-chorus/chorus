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
  dbPath: string;
  DatabaseCtor: new (path: string, opts: any) => {
    prepare: (sql: string) => { get: () => any };
    close?: () => void;
  };
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
        const db = new deps.DatabaseCtor(deps.dbPath, { readonly: true });
        const row = db.prepare('SELECT COUNT(*) as cnt FROM messages').get() as { cnt: number };
        state.dbRows = row.cnt;
        state.dbStatus = 'ok';
        try {
          const uRow = db.prepare(
            'SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= 100',
          ).get() as { cnt: number };
          state.unembedded = uRow.cnt;
        } catch {
          /* column may not exist yet */
        }
        db.close?.();
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
