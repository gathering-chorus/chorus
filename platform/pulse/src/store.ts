/**
 * Message Store — SQLite-backed persistent messaging (#1755)
 *
 * Replaces /tmp file queues with durable, queryable storage.
 * Single source of truth for nudges, chats, board events, role state.
 */

import Database from 'better-sqlite3';
import path from 'path';

const DB_PATH = path.join(__dirname, '..', 'messages.db');

export interface Message {
  id: number;
  type: 'nudge' | 'chat' | 'board-event' | 'role-state';
  from: string;
  to: string;
  content: string;
  chatId?: string;
  acknowledged: boolean;
  delivery_attempts: number;
  dead_letter: boolean;
  createdAt: string;
  acknowledgedAt?: string;
  dead_lettered_at?: string;
}

export class MessageStore {
  private db: Database.Database;

  constructor(dbPath?: string) {
    this.db = new Database(dbPath || DB_PATH);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');
    this.init();
  }

  private init(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT NOT NULL CHECK(type IN ('nudge', 'chat', 'board-event', 'role-state')),
        "from" TEXT NOT NULL,
        "to" TEXT NOT NULL,
        content TEXT NOT NULL,
        chat_id TEXT,
        acknowledged INTEGER DEFAULT 0,
        delivery_attempts INTEGER DEFAULT 0,
        dead_letter INTEGER DEFAULT 0,
        created_at TEXT DEFAULT (datetime('now')),
        acknowledged_at TEXT,
        dead_lettered_at TEXT
      );

      CREATE INDEX IF NOT EXISTS idx_messages_to ON messages("to", acknowledged);
      CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id) WHERE chat_id IS NOT NULL;
      CREATE INDEX IF NOT EXISTS idx_messages_type ON messages(type, created_at);

      -- #2632: role_state table retired. The HTTP role-state writer was a
      -- parallel implementation of the chorus-hook-shim CLI with zero
      -- callers across the codebase (probed during #2629). Single
      -- canonical address per the no-competing-implementations principle.
      -- DROP TABLE handled below to clean up any existing DB.

      CREATE TABLE IF NOT EXISTS chats (
        id TEXT PRIMARY KEY,
        role_a TEXT NOT NULL,
        role_b TEXT NOT NULL,
        topic TEXT,
        status TEXT DEFAULT 'active' CHECK(status IN ('active', 'ended')),
        created_at TEXT DEFAULT (datetime('now')),
        ended_at TEXT
      );
    `);

    // #2632 migration: drop the entire role_state table. Retirement step
    // — this writer was unused (zero callers) and parallel to the CLI.
    // DROP TABLE IF EXISTS is idempotent for fresh DBs (table never
    // created) and existing DBs alike. Safe to no-op after first run.
    try {
      this.db.exec(`DROP TABLE IF EXISTS role_state`);
    } catch {
      // tolerate any DB-level oddity — retirement is best-effort
    }

    // #2727 AC1: add delivery columns. Idempotent via PRAGMA table_info
    // guard. Existing rows backfill to 'delivered' (they predate the
    // worker and have already been surfaced via the retired path).
    const cols = this.db.prepare("PRAGMA table_info(messages)").all() as Array<{ name: string }>;
    const colNames = cols.map(c => c.name);
    if (!colNames.includes('delivery_status')) {
      this.db.exec(`ALTER TABLE messages ADD COLUMN delivery_status TEXT NOT NULL DEFAULT 'pending' CHECK(delivery_status IN ('pending','delivered','failed'))`);
      this.db.exec(`UPDATE messages SET delivery_status = 'delivered' WHERE id > 0`);
    }
    if (!colNames.includes('delivered_at')) {
      this.db.exec(`ALTER TABLE messages ADD COLUMN delivered_at TEXT`);
    }
    if (!colNames.includes('last_delivery_error')) {
      this.db.exec(`ALTER TABLE messages ADD COLUMN last_delivery_error TEXT`);
    }
    // #2765 AC2: trace_id correlation column. UUIDv7 minted at sender,
    // propagated via X-Chorus-Trace-Id header → row → every spine event.
    // Indexed for efficient join from chorus.log → messages.db row.
    if (!colNames.includes('trace_id')) {
      this.db.exec(`ALTER TABLE messages ADD COLUMN trace_id TEXT`);
    }
    this.db.exec(`CREATE INDEX IF NOT EXISTS idx_messages_delivery ON messages(delivery_status, type)`);
    this.db.exec(`CREATE INDEX IF NOT EXISTS idx_messages_trace_id ON messages(trace_id) WHERE trace_id IS NOT NULL`);
  }

  // --- Nudges ---

  sendNudge(from: string, to: string, content: string, traceId?: string): number {
    if (traceId) {
      const stmt = this.db.prepare(
        'INSERT INTO messages (type, "from", "to", content, trace_id) VALUES (\'nudge\', ?, ?, ?, ?)'
      );
      return Number(stmt.run(from, to, content, traceId).lastInsertRowid);
    }
    const stmt = this.db.prepare(
      'INSERT INTO messages (type, "from", "to", content) VALUES (\'nudge\', ?, ?, ?)'
    );
    return Number(stmt.run(from, to, content).lastInsertRowid);
  }

  // #2727 AC1: delivery state surface. Worker drives transitions via
  // markDelivered / markFailed; pulse boot scans getPendingDeliveries
  // for restart-requeue. getDeliveryRecord exposes failure detail.

  markDelivered(id: number): void {
    this.db.prepare(
      `UPDATE messages SET delivery_status = 'delivered', delivered_at = datetime('now') WHERE id = ?`
    ).run(id);
  }

  markFailed(id: number, reason: string): void {
    this.db.prepare(
      `UPDATE messages SET delivery_status = 'failed', last_delivery_error = ? WHERE id = ?`
    ).run(reason, id);
  }

  getPendingDeliveries(): Array<{ id: number; from: string; to: string; content: string; delivery_attempts: number; created_at: string; trace_id: string | null }> {
    return this.db.prepare(
      `SELECT id, "from" as "from", "to" as "to", content, delivery_attempts, created_at, trace_id FROM messages WHERE delivery_status = 'pending' AND type = 'nudge' ORDER BY id ASC`
    ).all() as Array<{ id: number; from: string; to: string; content: string; delivery_attempts: number; created_at: string; trace_id: string | null }>;
  }

  getDeliveryRecord(id: number): { delivery_status: string; delivered_at: string | null; last_delivery_error: string | null; delivery_attempts: number; trace_id: string | null } {
    const row = this.db.prepare(
      `SELECT delivery_status, delivered_at, last_delivery_error, delivery_attempts, trace_id FROM messages WHERE id = ?`
    ).get(id);
    if (!row) {
      throw new Error(`getDeliveryRecord: no row for id=${id}`);
    }
    return row as { delivery_status: string; delivered_at: string | null; last_delivery_error: string | null; delivery_attempts: number; trace_id: string | null };
  }

  // #2664: getPendingNudges, recordDeliveryAttempt, getDeadLetters,
  // replayDeadLetter, acknowledgeNudge, acknowledgeAllNudges retired.
  // Delivery confirmation is the nudge.surfaced spine event (DEC-107
  // canonical persist + osascript inject; receipts via spine fold).
  // The acknowledged / delivery_attempts / dead_letter columns remain
  // in the schema as vestigial — write-once-zero — to keep the migration
  // forward-only. A column-drop pass is its own card.

  // --- Chats ---

  startChat(roleA: string, roleB: string, topic: string): string {
    const id = `${roleA}-${roleB}-${Date.now()}`;
    this.db.prepare(
      'INSERT INTO chats (id, role_a, role_b, topic) VALUES (?, ?, ?, ?)'
    ).run(id, roleA, roleB, topic);
    return id;
  }

  chatMessage(chatId: string, from: string, content: string): number {
    const to = this.getChatPartner(chatId, from);
    const stmt = this.db.prepare(
      'INSERT INTO messages (type, "from", "to", content, chat_id) VALUES (\'chat\', ?, ?, ?, ?)'
    );
    return Number(stmt.run(from, to, content, chatId).lastInsertRowid);
  }

  getChatMessages(chatId: string, sinceId?: number): Message[] {
    if (sinceId) {
      return this.db.prepare(
        'SELECT * FROM messages WHERE chat_id = ? AND id > ? ORDER BY created_at'
      ).all(chatId, sinceId) as Message[];
    }
    return this.db.prepare(
      'SELECT * FROM messages WHERE chat_id = ? ORDER BY created_at'
    ).all(chatId) as Message[];
  }

  endChat(chatId: string): void {
    this.db.prepare(
      'UPDATE chats SET status = \'ended\', ended_at = datetime(\'now\') WHERE id = ?'
    ).run(chatId);
  }

  private getChatPartner(chatId: string, from: string): string {
    const chat = this.db.prepare('SELECT role_a, role_b FROM chats WHERE id = ?').get(chatId) as { role_a: string; role_b: string } | undefined;
    if (!chat) return 'unknown';
    return chat.role_a === from ? chat.role_b : chat.role_a;
  }

  // --- Board Events ---

  recordBoardEvent(from: string, content: string): number {
    const stmt = this.db.prepare(
      'INSERT INTO messages (type, "from", "to", content) VALUES (\'board-event\', ?, \'all\', ?)'
    );
    return Number(stmt.run(from, content).lastInsertRowid);
  }

  // --- Role State ---
  //
  // #2632: setRoleState / getRoleState / role_state table retired. Pulse's
  // HTTP role-state writer was parallel to chorus-hook-shim CLI with zero
  // callers across the codebase — no-competing-implementations applied.
  // Source of truth: chorus-hook-shim role-state subcommand →
  // /tmp/claude-team-scan/<role>-declared.json.

  // --- Queries ---

  queryMessages(opts: { type?: string; from?: string; to?: string; since?: string; limit?: number }): Message[] {
    let sql = 'SELECT * FROM messages WHERE 1=1';
    const params: (string | number)[] = [];

    if (opts.type) { sql += ' AND type = ?'; params.push(opts.type); }
    if (opts.from) { sql += ' AND "from" = ?'; params.push(opts.from); }
    if (opts.to) { sql += ' AND "to" = ?'; params.push(opts.to); }
    if (opts.since) { sql += ' AND created_at >= ?'; params.push(opts.since); }

    sql += ' ORDER BY created_at DESC';
    if (opts.limit) { sql += ' LIMIT ?'; params.push(opts.limit); }

    return this.db.prepare(sql).all(...params) as Message[];
  }

  // --- Stats ---

  getStats(): { total: number; pending: number; byType: Record<string, number> } {
    const total = (this.db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number }).c;
    const pending = (this.db.prepare('SELECT COUNT(*) as c FROM messages WHERE acknowledged = 0').get() as { c: number }).c;
    const rows = this.db.prepare('SELECT type, COUNT(*) as c FROM messages GROUP BY type').all() as Array<{ type: string; c: number }>;
    const byType: Record<string, number> = {};
    for (const row of rows) byType[row.type] = row.c;
    return { total, pending, byType };
  }

  close(): void {
    this.db.close();
  }
}
