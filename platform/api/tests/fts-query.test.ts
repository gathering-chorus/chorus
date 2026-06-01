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

// #3171 — the context-inject queries mode=hybrid. Mixed-source fixture: a knowledge
// doc (OLDER) vs session chatter (NEWER). Recency would surface the chatter; authority
// must surface the doc. This is the bug the inject hit (candidates were jeff/wren chatter).
function makeMixedSourceDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT, channel TEXT,
      role TEXT, author TEXT, content TEXT, timestamp TEXT);
    CREATE VIRTUAL TABLE messages_fts USING fts5(content);
  `);
  const rows = [
    // knowledge doc — OLDEST (loses on recency, must win on authority)
    { id: 1, source: 'doc',    content: 'heidegger versammlung gathering philosophy', timestamp: '2026-01-01T00:00:00Z' },
    // session chatter — NEWER (wins on recency, must lose on authority)
    { id: 2, source: 'claude', content: 'can u run the heidegger search again',      timestamp: '2026-06-01T00:00:00Z' },
    { id: 3, source: 'claude', content: 'ran the heidegger query here is the log',   timestamp: '2026-06-01T01:00:00Z' },
  ];
  const ins = db.prepare(
    `INSERT INTO messages (id, source, channel, role, author, content, timestamp)
     VALUES (@id, @source, 'session', 'wren', 'assistant', @content, @timestamp)`,
  );
  const insF = db.prepare('INSERT INTO messages_fts (rowid, content) VALUES (?, ?)');
  for (const r of rows) { ins.run(r); insF.run(r.id, r.content); }
  return db;
}

describe('runFtsQueryOnDb — #3171 authority ranking reaches hybrid (the inject query)', () => {
  it('hybrid mode ranks a knowledge doc above NEWER session chatter (authority, not recency)', () => {
    const db = makeMixedSourceDb();
    const rows = runFtsQueryOnDb(db, 'heidegger', 10, undefined, 'hybrid') as Array<{ id: number; source: string }>;
    // pre-#3171: hybrid → timestamp DESC → newest chatter (id 3) first. authority must surface the doc.
    expect(rows[0].source).toBe('doc');
    expect(rows[0].id).toBe(1);
    db.close();
  });

  it('relevance mode still ranks the doc above chatter (unchanged by #3171)', () => {
    const db = makeMixedSourceDb();
    const rows = runFtsQueryOnDb(db, 'heidegger', 10, undefined, 'relevance') as Array<{ source: string }>;
    expect(rows[0].source).toBe('doc');
    db.close();
  });

  it('recency mode stays pure-recency — #3171 must NOT change the conversation-rebuild path', () => {
    const db = makeMixedSourceDb();
    const rows = runFtsQueryOnDb(db, 'heidegger', 10, undefined, 'recency') as Array<{ id: number }>;
    expect(rows[0].id).toBe(3); // newest chatter first — correct for recency
    db.close();
  });
});
