// embed-at-ingest worker (extracted from server.ts for #2205 wave 16).
//
// Pages through unembedded messages in SQLite, calls Ollama for each,
// writes the batch to LanceDB, and marks the rows as embedded. Deps are
// injected so tests run with fake Database + fake embedder + fake lance
// store.

/** Prepared-statement method — any used because better-sqlite3 Statement<T>
 *  is too strict for structural shim typing. Callers cast results. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type StmtMethod = (...args: any[]) => any;

/** Lance record — arbitrary keyed row (field shape varies by table). */
type LanceRow = Record<string, unknown>;

export interface EmbedDeltaDeps {
  dbPath: string;
  DatabaseCtor: new (path: string, opts?: { readonly?: boolean }) => {
    pragma: (s: string) => void;
    exec: (sql: string) => void;
    prepare: (sql: string) => {
      all?: StmtMethod;
      get?: StmtMethod;
      run?: StmtMethod;
    };
    transaction: (fn: (ids: number[]) => void) => (ids: number[]) => void;
    close: () => void;
  };
  getLanceStore: () => {
    db: { createTable: (n: string, rec: LanceRow[]) => Promise<unknown> } | null;
    table: { add: (rec: LanceRow[]) => Promise<void> } | null;
  };
  setLanceTable: (t: unknown) => void;
  embed: (text: string) => Promise<number[]>;
  minLength: number;
  pageSize: number;
  log?: (m: string) => void;
  error?: (m: string) => void;
}

export interface EmbedDeltaResult {
  embedded: number;
  skipped: number;
  ollama_failures: number;
}

export function createEmbedDelta(deps: EmbedDeltaDeps): () => Promise<EmbedDeltaResult> {
  const log = deps.log ?? ((m) => console.log(m));
  const error = deps.error ?? ((m) => console.error(m));
  // #2627: schema-migration + per-msg embed + lance-store + mark-embedded
  // each get a helper; orchestrator becomes a linear pipeline.
  type EmbedRecord = {
    msg_id: number; source: string; channel: string; role: string;
    content: string; timestamp: string; vector: number[];
  };
  type Msg = { id: number; source: string; channel: string; role: string; content: string; timestamp: string };

  function ensureEmbeddedColumn(): void {
    const rwDb = new deps.DatabaseCtor(deps.dbPath);
    rwDb.pragma('journal_mode = WAL');
    try { rwDb.exec('ALTER TABLE messages ADD COLUMN embedded INTEGER DEFAULT 0'); } catch { /* exists */ }
    rwDb.close();
  }

  async function embedPage(page: Msg[]): Promise<{ records: EmbedRecord[]; skipped: number; ollamaFailures: number }> {
    const records: EmbedRecord[] = [];
    let skipped = 0;
    let ollamaFailures = 0;
    for (const msg of page) {
      try {
        const text = `[${msg.source}/${msg.role}] ${msg.content.slice(0, 2000)}`;
        const vector = await deps.embed(text);
        records.push({
          msg_id: msg.id, source: msg.source, channel: msg.channel, role: msg.role,
          content: msg.content.slice(0, 2000), timestamp: msg.timestamp, vector,
        });
      } catch (err: unknown) {
        skipped++;
        ollamaFailures++;
        error(`[embed-delta] Ollama failure for msg ${msg.id}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
    return { records, skipped, ollamaFailures };
  }

  async function persistToLance(records: EmbedRecord[]): Promise<void> {
    const store = deps.getLanceStore();
    if (store.table) {
      await store.table.add(records);
    } else if (store.db) {
      const created = await store.db.createTable('messages', records);
      deps.setLanceTable(created);
    }
  }

  function markEmbedded(records: EmbedRecord[]): void {
    const markDb = new deps.DatabaseCtor(deps.dbPath);
    markDb.pragma('journal_mode = WAL');
    const markStmt = markDb.prepare('UPDATE messages SET embedded = 1 WHERE id = ?');
    const markMany = markDb.transaction((ids: number[]) => {
      for (const id of ids) markStmt.run!(id);
    });
    markMany(records.map(r => r.msg_id));
    markDb.close();
  }

  return async function embedDelta(): Promise<EmbedDeltaResult> {
    const db = new deps.DatabaseCtor(deps.dbPath, { readonly: true });
    db.pragma('journal_mode = WAL');
    try {
      ensureEmbeddedColumn();
      const page = db.prepare(`
        SELECT id, source, channel, role, content, timestamp
        FROM messages
        WHERE embedded = 0 AND LENGTH(content) >= ?
        ORDER BY id ASC
        LIMIT ?
      `).all!(deps.minLength, deps.pageSize) as Msg[];
      if (page.length === 0) return { embedded: 0, skipped: 0, ollama_failures: 0 };
      const countRow = db.prepare(`
        SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= ?
      `).get!(deps.minLength) as { cnt: number };
      const { records, skipped, ollamaFailures } = await embedPage(page);
      if (records.length === 0) return { embedded: 0, skipped, ollama_failures: ollamaFailures };
      await persistToLance(records);
      markEmbedded(records);
      const tail = ollamaFailures > 0 ? `, ollama_failures ${ollamaFailures}` : '';
      log(`[embed-delta] Embedded ${records.length}/${countRow.cnt} remaining, skipped ${skipped}${tail}`);
      return { embedded: records.length, skipped, ollama_failures: ollamaFailures };
    } finally {
      db.close();
    }
  };
}
