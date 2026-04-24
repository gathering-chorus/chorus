// Lazy table-ensurers for RCA + trace tables (extracted from server.ts
// for #2205 wave 14).
//
// Factory form so tests stub the Database constructor and observe what
// SQL got executed.

/** Minimal Database surface used by table-ensurers. Structurally satisfied
 *  by better-sqlite3's Database and by test stubs. (#2463 wave 1 widened the
 *  constructor signature + prepare() return to remove `as any` casts at call sites.) */
/** Minimal Database handle surface — structurally satisfied by better-sqlite3. */
export interface DbHandle {
  pragma: (s: string) => void;
  exec: (sql: string) => void;
  prepare: (sql: string) => { get: () => unknown };
  close: () => void;
}

/** Constructor shape: covariant in the handle, tolerant of additional options
 *  via a rest parameter — lets `typeof Database` (which accepts Options) assign. */
export interface TableEnsurerDeps {
  dbPath: string;
  DatabaseCtor: new (path: string, ...rest: never[]) => DbHandle;
}

export function createRcaTableEnsurer(deps: TableEnsurerDeps): () => void {
  return () => {
    const db = new deps.DatabaseCtor(deps.dbPath);
    db.pragma('journal_mode = WAL');
    db.exec(`CREATE TABLE IF NOT EXISTS rcas (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      title TEXT NOT NULL,
      trigger_event TEXT NOT NULL,
      timeline TEXT,
      root_cause TEXT NOT NULL,
      contributing_factors TEXT DEFAULT '[]',
      corrective_actions TEXT DEFAULT '[]',
      cards TEXT DEFAULT '[]',
      spine_events TEXT DEFAULT '[]',
      status TEXT DEFAULT 'open',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    )`);
    db.close();
  };
}

export function createTraceTableEnsurer(deps: TableEnsurerDeps): () => void {
  return () => {
    const db = new deps.DatabaseCtor(deps.dbPath);
    db.pragma('journal_mode = WAL');
    db.exec(`CREATE TABLE IF NOT EXISTS traces (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      correlation_id TEXT NOT NULL,
      hop INTEGER NOT NULL,
      call_stack TEXT NOT NULL,
      source_domain TEXT,
      source_service TEXT,
      source_instance TEXT,
      dest_domain TEXT,
      dest_service TEXT,
      dest_instance TEXT,
      timestamp TEXT NOT NULL,
      latency_ms INTEGER,
      error_class TEXT,
      error_message TEXT,
      created_at TEXT NOT NULL
    )`);
    const hasIdx = db.prepare("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_traces_corr'").get();
    if (!hasIdx) {
      db.exec('CREATE INDEX idx_traces_corr ON traces(correlation_id)');
      db.exec('CREATE INDEX idx_traces_domain ON traces(source_domain)');
    }
    db.close();
  };
}
