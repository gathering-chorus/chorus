/**
 * #3086 — characterization test for the shared FTS query (lib/fts-query).
 * Locks today's FTS output + ordering so the worker_threads offload (which imports
 * the SAME runFtsQueryOnDb) is parity-by-construction. AC: "response shape + ranking
 * identical to today." Written before the module exists (TDD red → green).
 */
import Database from 'better-sqlite3';
import { toFtsMatchQuery, runFtsQueryOnDb } from '../src/lib/fts-query';

function makeDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT, channel TEXT,
      role TEXT, author TEXT, content TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content);
  `);
  const rows = [
    { id: 1, role: 'wren',  content: 'event loop blocked by search',        timestamp: '2026-05-25T10:00:00Z' },
    { id: 2, role: 'silas', content: 'reindex worker off the loop',         timestamp: '2026-05-25T11:00:00Z' },
    { id: 3, role: 'wren',  content: 'search offload via worker threads',   timestamp: '2026-05-25T12:00:00Z' },
  ];
  const ins = db.prepare(
    `INSERT INTO messages (id, source, channel, role, author, content, timestamp)
     VALUES (@id, 'claude', 'session', @role, 'assistant', @content, @timestamp)`,
  );
  const insF = db.prepare('INSERT INTO messages_fts (rowid, content) VALUES (?, ?)');
  for (const r of rows) { ins.run(r); insF.run(r.id, r.content); }
  return db;
}

describe('toFtsMatchQuery', () => {
  it('quotes word tokens and drops punctuation', () => {
    expect(toFtsMatchQuery('event loop!')).toBe('"event" "loop"');
  });
  it('returns empty string when there are no word tokens', () => {
    expect(toFtsMatchQuery('!!! ??? ')).toBe('');
  });
});

describe('runFtsQueryOnDb', () => {
  it('returns matching rows, newest-first by default (timestamp DESC)', () => {
    const db = makeDb();
    const rows = runFtsQueryOnDb(db, 'search', 10, undefined, 'fts') as Array<{ id: number; snippet: string }>;
    expect(rows.map((r) => r.id)).toEqual([3, 1]);
    expect(rows[0].snippet).toContain('<b>search</b>');
    db.close();
  });

  it('applies the role filter', () => {
    const db = makeDb();
    const rows = runFtsQueryOnDb(db, 'loop', 10, 'silas', 'fts') as Array<{ id: number }>;
    expect(rows.map((r) => r.id)).toEqual([2]);
    db.close();
  });

  it('honors fetchLimit', () => {
    const db = makeDb();
    expect(runFtsQueryOnDb(db, 'worker', 10, undefined, 'fts')).toHaveLength(2);
    expect(runFtsQueryOnDb(db, 'worker', 1, undefined, 'fts')).toHaveLength(1);
    db.close();
  });

  it('returns [] for a query with no word tokens (never scans)', () => {
    const db = makeDb();
    expect(runFtsQueryOnDb(db, '!!!', 10, undefined, 'fts')).toEqual([]);
    db.close();
  });
});
