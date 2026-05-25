import Database from 'better-sqlite3';

/** Small in-memory messages + messages_fts fixture for #3086 search tests. */
export function makeFtsDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT, channel TEXT,
      role TEXT, author TEXT, content TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content);
  `);
  const rows = [
    { id: 1, role: 'wren',  content: 'event loop blocked by search',      timestamp: '2026-05-25T10:00:00Z' },
    { id: 2, role: 'silas', content: 'reindex worker off the loop',       timestamp: '2026-05-25T11:00:00Z' },
    { id: 3, role: 'wren',  content: 'search offload via worker threads',  timestamp: '2026-05-25T12:00:00Z' },
  ];
  const ins = db.prepare(
    `INSERT INTO messages (id, source, channel, role, author, content, timestamp)
     VALUES (@id, 'claude', 'session', @role, 'assistant', @content, @timestamp)`,
  );
  const insF = db.prepare('INSERT INTO messages_fts (rowid, content) VALUES (?, ?)');
  for (const r of rows) { ins.run(r); insF.run(r.id, r.content); }
  return db;
}
