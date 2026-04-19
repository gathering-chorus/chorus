// embed-at-ingest worker (extracted from server.ts for #2205 wave 16).
//
// Pages through unembedded messages in SQLite, calls Ollama for each,
// writes the batch to LanceDB, and marks the rows as embedded. Deps are
// injected so tests run with fake Database + fake embedder + fake lance
// store.

export interface EmbedDeltaDeps {
  dbPath: string;
  DatabaseCtor: new (path: string, opts?: any) => {
    pragma: (s: string) => void;
    exec: (sql: string) => void;
    prepare: (sql: string) => {
      all?: (...args: any[]) => any[];
      get?: (...args: any[]) => any;
      run?: (...args: any[]) => any;
    };
    transaction: (fn: (ids: number[]) => void) => (ids: number[]) => void;
    close: () => void;
  };
  getLanceStore: () => { db: { createTable: (n: string, rec: any[]) => Promise<any> } | null; table: { add: (rec: any[]) => Promise<void> } | null };
  setLanceTable: (t: any) => void;
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
  return async function embedDelta(): Promise<EmbedDeltaResult> {
    const db = new deps.DatabaseCtor(deps.dbPath, { readonly: true });
    db.pragma('journal_mode = WAL');
    try {
      // Schema check — add embedded column if missing (#1920).
      const rwDb = new deps.DatabaseCtor(deps.dbPath);
      rwDb.pragma('journal_mode = WAL');
      try {
        rwDb.exec(`ALTER TABLE messages ADD COLUMN embedded INTEGER DEFAULT 0`);
      } catch {
        /* column already exists */
      }
      rwDb.close();

      const page = db.prepare(`
        SELECT id, source, channel, role, content, timestamp
        FROM messages
        WHERE embedded = 0 AND LENGTH(content) >= ?
        ORDER BY id ASC
        LIMIT ?
      `).all!(deps.minLength, deps.pageSize) as Array<{
        id: number; source: string; channel: string; role: string; content: string; timestamp: string;
      }>;

      if (page.length === 0) return { embedded: 0, skipped: 0, ollama_failures: 0 };

      const countRow = db.prepare(`
        SELECT COUNT(*) as cnt FROM messages WHERE embedded = 0 AND LENGTH(content) >= ?
      `).get!(deps.minLength) as { cnt: number };

      const records: Array<{
        msg_id: number; source: string; channel: string; role: string;
        content: string; timestamp: string; vector: number[];
      }> = [];
      let skipped = 0;
      let ollamaFailures = 0;

      for (const msg of page) {
        try {
          const text = `[${msg.source}/${msg.role}] ${msg.content.slice(0, 2000)}`;
          const vector = await deps.embed(text);
          records.push({
            msg_id: msg.id,
            source: msg.source,
            channel: msg.channel,
            role: msg.role,
            content: msg.content.slice(0, 2000),
            timestamp: msg.timestamp,
            vector,
          });
        } catch (err: any) {
          skipped++;
          ollamaFailures++;
          error(`[embed-delta] Ollama failure for msg ${msg.id}: ${err.message}`);
        }
      }

      if (records.length === 0) return { embedded: 0, skipped, ollama_failures: ollamaFailures };

      const store = deps.getLanceStore();
      if (store.table) {
        await store.table.add(records);
      } else if (store.db) {
        const created = await store.db.createTable('messages', records);
        deps.setLanceTable(created);
      }

      const markDb = new deps.DatabaseCtor(deps.dbPath);
      markDb.pragma('journal_mode = WAL');
      const markStmt = markDb.prepare(`UPDATE messages SET embedded = 1 WHERE id = ?`);
      const markMany = markDb.transaction((ids: number[]) => {
        for (const id of ids) markStmt.run!(id);
      });
      markMany(records.map(r => r.msg_id));
      markDb.close();

      if (ollamaFailures > 0) {
        log(`[embed-delta] Embedded ${records.length}/${countRow.cnt} remaining, skipped ${skipped}, ollama_failures ${ollamaFailures}`);
      } else {
        log(`[embed-delta] Embedded ${records.length}/${countRow.cnt} remaining, skipped ${skipped}`);
      }
      return { embedded: records.length, skipped, ollama_failures: ollamaFailures };
    } finally {
      db.close();
    }
  };
}
