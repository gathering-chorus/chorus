#!/usr/bin/env bash
# chorus-init-db.sh — Initialize the shared memory SQLite database
# Part of the Chorus context service (DEC-030, Wren's vertical)
set -euo pipefail

DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"

if [ -f "$DB_PATH" ]; then
  echo "Database already exists at $DB_PATH"
  echo "Tables:"
  sqlite3 "$DB_PATH" ".tables"
  echo "Message count:"
  sqlite3 "$DB_PATH" "SELECT source, COUNT(*) FROM messages GROUP BY source;"
  exit 0
fi

echo "Creating database at $DB_PATH..."

sqlite3 "$DB_PATH" <<'SQL'
-- Messages from all sources (Slack, Claude sessions, artifacts)
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,        -- 'slack', 'claude', 'brief', 'decision', 'activity'
    source_id TEXT UNIQUE,       -- slack ts+channel, session UUID+offset, file path
    channel TEXT,                -- slack channel or 'session:wren', 'brief:silas'
    role TEXT,                   -- wren, silas, kade, jeff, system, unknown
    author TEXT,                 -- slack user ID or 'user'/'assistant'
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,     -- ISO 8601
    session_id TEXT,             -- claude session UUID (null for slack)
    thread_id TEXT,              -- slack thread_ts or conversation_id
    is_bridge INTEGER DEFAULT 0,
    metadata TEXT                -- JSON blob for extras
);

-- Full-text search index
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    role,
    channel,
    content='messages',
    content_rowid='id'
);

-- Keep FTS in sync with inserts
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, role, channel)
    VALUES (new.id, new.content, new.role, new.channel);
END;

-- Keep FTS in sync with deletes
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content, role, channel)
    VALUES ('delete', old.id, old.content, old.role, old.channel);
END;

-- Watermarks for incremental indexing
CREATE TABLE IF NOT EXISTS watermarks (
    source TEXT PRIMARY KEY,     -- 'slack:all-gathering', 'claude:product-manager'
    last_seen TEXT NOT NULL,     -- timestamp or cursor
    last_indexed TEXT NOT NULL   -- ISO 8601 when we last ran
);

-- Index for common queries
CREATE INDEX IF NOT EXISTS idx_messages_source ON messages(source);
CREATE INDEX IF NOT EXISTS idx_messages_role ON messages(role);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);

-- References table for entity cross-referencing (WF-004, card #123)
CREATE TABLE IF NOT EXISTS refs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id INTEGER NOT NULL,
    entity_type TEXT NOT NULL,        -- 'card', 'workflow', 'decision', 'adr', 'brief', 'message'
    entity_id TEXT NOT NULL,          -- '#115', 'WF-003', 'DEC-035', 'ADR-012'
    relationship TEXT NOT NULL DEFAULT 'mentions',  -- 'mentions', 'responds_to', 'confirms', 'blocks', 'resolves'
    extracted_at TEXT NOT NULL,       -- ISO 8601
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refs_message ON refs(message_id);
CREATE INDEX IF NOT EXISTS idx_refs_entity ON refs(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_refs_relationship ON refs(relationship);
SQL

echo "Database initialized."
sqlite3 "$DB_PATH" ".tables"
