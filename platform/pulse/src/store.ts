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

      -- #2467 / #2629: card field removed. Card lives on the board, not
      -- in pulse role_state. Migration for existing DBs handled below.
      CREATE TABLE IF NOT EXISTS role_state (
        role TEXT PRIMARY KEY,
        state TEXT NOT NULL,
        detail TEXT,
        updated_at TEXT DEFAULT (datetime('now'))
      );

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

    // #2629 migration: drop role_state.card column from any existing DB.
    // SQLite ≥3.35 supports ALTER TABLE DROP COLUMN. Wrapped because
    // (a) fresh DBs created with the new schema above don't have the
    // column, (b) DBs already migrated don't have it either. Both cases
    // make ALTER fail; we tolerate that.
    try {
      this.db.exec(`ALTER TABLE role_state DROP COLUMN card`);
    } catch {
      // already migrated or never had the column — both fine
    }
  }

  // --- Nudges ---

  sendNudge(from: string, to: string, content: string): number {
    const stmt = this.db.prepare(
      'INSERT INTO messages (type, "from", "to", content) VALUES (\'nudge\', ?, ?, ?)'
    );
    return Number(stmt.run(from, to, content).lastInsertRowid);
  }

  getPendingNudges(role: string): Message[] {
    const stmt = this.db.prepare(
      'SELECT * FROM messages WHERE type = \'nudge\' AND "to" = ? AND acknowledged = 0 AND dead_letter = 0 ORDER BY created_at'
    );
    return stmt.all(role) as Message[];
  }

  /** Record a delivery attempt. After MAX_ATTEMPTS, move to dead-letter. */
  recordDeliveryAttempt(id: number): { deadLettered: boolean } {
    const MAX_ATTEMPTS = 3;
    this.db.prepare('UPDATE messages SET delivery_attempts = delivery_attempts + 1 WHERE id = ?').run(id);
    const row = this.db.prepare('SELECT delivery_attempts FROM messages WHERE id = ?').get(id) as { delivery_attempts: number } | undefined;
    if (row && row.delivery_attempts >= MAX_ATTEMPTS) {
      this.db.prepare('UPDATE messages SET dead_letter = 1, dead_lettered_at = datetime(\'now\') WHERE id = ?').run(id);
      return { deadLettered: true };
    }
    return { deadLettered: false };
  }

  getDeadLetters(opts?: { limit?: number }): Message[] {
    const limit = opts?.limit || 50;
    return this.db.prepare(
      'SELECT * FROM messages WHERE dead_letter = 1 ORDER BY dead_lettered_at DESC LIMIT ?'
    ).all(limit) as Message[];
  }

  /** Replay a dead-lettered message — reset attempts and dead-letter flag */
  replayDeadLetter(id: number): void {
    this.db.prepare(
      'UPDATE messages SET dead_letter = 0, dead_lettered_at = NULL, delivery_attempts = 0 WHERE id = ?'
    ).run(id);
  }

  acknowledgeNudge(id: number): void {
    this.db.prepare(
      'UPDATE messages SET acknowledged = 1, acknowledged_at = datetime(\'now\') WHERE id = ?'
    ).run(id);
  }

  acknowledgeAllNudges(role: string): number {
    const result = this.db.prepare(
      'UPDATE messages SET acknowledged = 1, acknowledged_at = datetime(\'now\') WHERE type = \'nudge\' AND "to" = ? AND acknowledged = 0'
    ).run(role);
    return result.changes;
  }

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

  // #2467 / #2629: card removed. Card lives on the board — pulse stores
  // session/attention metadata only. Callers must not pass card.
  setRoleState(role: string, state: string, detail?: string): void {
    this.db.prepare(`
      INSERT INTO role_state (role, state, detail, updated_at)
      VALUES (?, ?, ?, datetime('now'))
      ON CONFLICT(role) DO UPDATE SET
        state = excluded.state,
        detail = excluded.detail,
        updated_at = excluded.updated_at
    `).run(role, state, detail || null);
  }

  getRoleState(role: string): { state: string; detail: string | null; updatedAt: string } | null {
    const row = this.db.prepare('SELECT role, state, detail, updated_at FROM role_state WHERE role = ?').get(role) as { state: string; detail: string | null; updated_at: string } | undefined;
    if (!row) return null;
    return { state: row.state, detail: row.detail, updatedAt: row.updated_at };
  }

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
