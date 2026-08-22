// #3959 — index the spine into a queryable store.
//
// The spine (~/.chorus/chorus.log) is the FIRST and ONLY write: every emitter
// appends a JSON line (membrane.rs:246), and it is never rotated (Jeff's ruling,
// 2026-07-23). It is durable, 1.2 GB, 1,511,664 lines — and greppable only. The
// one queryable copy, Loki, carries `retention_period: 168h` with deletes
// enabled, so anything older than a week is gone.
//
// Jeff, 2026-08-21: "we could do ml on our actions if we had the fucking data."
// The data exists. What is missing is an index over it.
//
// THIS IS NOT A SECOND CAPTURE PATH. Nothing emits here — this reads the spine
// and projects it, so the system still has exactly one write. Resumable by byte
// offset, so a re-run costs only the new bytes.
//
// One file, two engines. SQLite owns the writes (WAL) because our emitters are
// dozens of short-lived processes, which is precisely what DuckDB's
// single-writer-process model cannot take. DuckDB attaches the SAME file for
// columnar reads when we want ML:
//
//   INSTALL sqlite; LOAD sqlite;
//   SELECT role, count(*) FROM sqlite_scan('streams.db','events') GROUP BY role;
//
// Pattern proven by platform/pulse/messages.db: same box, same engine, 27,965
// rows over four and a half months, 29 MB, nothing deleted.

import Database from 'better-sqlite3';
import { openSync, fstatSync, readSync, closeSync } from 'fs';

export interface IndexResult {
  /** Rows inserted this run. */
  inserted: number;
  /** Lines read but not parseable as JSON. */
  malformed: number;
  /** Byte offset the indexer stopped at — the resume point. */
  offset: number;
  /** Rows whose role is not one of the three known roles. */
  unattributed: number;
  /** True when bytes remain past this chunk — call again to continue. */
  more: boolean;
}

export interface AttributionRate {
  total: number;
  attributed: number;
  /** attributed / total, 0..1. The number Jeff should never have to ask for. */
  rate: number;
  byRole: Record<string, number>;
}

const KNOWN_ROLES = new Set(['wren', 'silas', 'kade']);

/** Create the schema if absent. WAL because many readers, one writer. */
export function openStreamsDb(path: string): Database.Database {
  const db = new Database(path);
  db.pragma('journal_mode = WAL');
  db.exec(`
    CREATE TABLE IF NOT EXISTS events (
      id        INTEGER PRIMARY KEY,
      ts        TEXT NOT NULL,
      role      TEXT NOT NULL,
      event     TEXT NOT NULL,
      tool      TEXT,
      phase     TEXT,
      card_id   TEXT,
      trace_id  TEXT,
      payload   TEXT NOT NULL
    );
    -- the two questions actually asked: "what did <role> do between t1 and t2"
    -- and "what happened on this trace".
    CREATE INDEX IF NOT EXISTS events_role_ts ON events(role, ts);
    CREATE INDEX IF NOT EXISTS events_ts      ON events(ts);
    CREATE INDEX IF NOT EXISTS events_trace   ON events(trace_id);
    CREATE TABLE IF NOT EXISTS ingest_state (
      source TEXT PRIMARY KEY,
      offset INTEGER NOT NULL
    );
  `);
  return db;
}

/** How much of the spine to convert to string at once.
 *
 *  Found by running this against the real 1.2 GB spine, not by review: a single
 *  `buf.toString()` over the whole backlog threw
 *  "Cannot create a string longer than 0x1fffffe8 characters" — V8's ~512 MB
 *  string ceiling. The fixture tests all passed, because fixtures are small.
 *  That is exactly the seam-vs-unit gap this card is about. */
export const READ_CHUNK_BYTES = 64 * 1024 * 1024;

/**
 * Read from `from` toward EOF, bounded to one chunk. Returns the text, the file
 * size, and whether more remains — so a large backlog drains over successive
 * calls instead of dying on the first.
 */
function readFrom(
  path: string,
  from: number,
): { text: string; size: number; readTo: number; more: boolean } {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- caller-supplied spine path, not user input (#3959)
  const fd = openSync(path, 'r');
  try {
    const size = fstatSync(fd).size;
    // A spine that SHRANK was rotated or replaced. Never resume mid-file against
    // different content — restart from zero rather than silently indexing
    // whatever now sits at the old offset.
    const start = from > size ? 0 : from;
    const remaining = size - start;
    if (remaining <= 0) return { text: '', size, readTo: size, more: false };
    const len = Math.min(remaining, READ_CHUNK_BYTES);
    const buf = Buffer.allocUnsafe(len);
    readSync(fd, buf, 0, len, start);
    return {
      text: buf.toString('utf8'),
      size,
      readTo: start + len,
      more: start + len < size,
    };
  } finally {
    closeSync(fd);
  }
}

/**
 * Index new spine lines into `db`. Resumable: picks up at the stored offset.
 *
 * A partial final line is NOT indexed — the offset rewinds to the last newline
 * so the next run reads it whole. Counting half a JSON object as malformed
 * would make a healthy growing tail look like corruption.
 */
export function indexSpine(
  db: Database.Database,
  spinePath: string,
  opts: { batchLimit?: number } = {},
): IndexResult {
  const row = db.prepare('SELECT offset FROM ingest_state WHERE source = ?').get(spinePath) as
    | { offset: number }
    | undefined;
  const from = row?.offset ?? 0;
  const { text, size, more } = readFrom(spinePath, from);
  if (!text) return { inserted: 0, malformed: 0, offset: size, unattributed: 0, more: false };

  const restarted = from > size;
  const base = restarted ? 0 : from;
  const lastNl = text.lastIndexOf('\n');
  const complete = lastNl >= 0 ? text.slice(0, lastNl) : '';
  const consumed = base + Buffer.byteLength(lastNl >= 0 ? text.slice(0, lastNl + 1) : '', 'utf8');

  const insert = db.prepare(`
    INSERT INTO events (ts, role, event, tool, phase, card_id, trace_id, payload)
    VALUES (@ts, @role, @event, @tool, @phase, @card_id, @trace_id, @payload)
  `);

  let inserted = 0;
  let malformed = 0;
  let unattributed = 0;
  const limit = opts.batchLimit ?? Number.POSITIVE_INFINITY;

  const run = db.transaction((lines: string[]) => {
    for (const line of lines) {
      if (inserted >= limit) break;
      if (!line.trim()) continue;
      let d: Record<string, unknown>;
      try {
        d = JSON.parse(line) as Record<string, unknown>;
      } catch {
        malformed += 1;
        continue;
      }
      const role = typeof d.role === 'string' ? d.role : '';
      if (!KNOWN_ROLES.has(role)) unattributed += 1;
      insert.run({
        ts: typeof d.timestamp === 'string' ? d.timestamp : '',
        role,
        event: typeof d.event === 'string' ? d.event : '',
        tool: typeof d.tool === 'string' ? d.tool : null,
        phase: typeof d.phase === 'string' ? d.phase : null,
        // String(unknown) can stringify an object to "[object Object]" — a
        // card_id that is silently garbage is worse than one that is absent.
        card_id:
          typeof d.card_id === 'string' || typeof d.card_id === 'number'
            ? String(d.card_id)
            : null,
        trace_id: typeof d.trace_id === 'string' ? d.trace_id : null,
        payload: line,
      });
      inserted += 1;
    }
  });

  run(complete.split('\n'));

  db.prepare(
    'INSERT INTO ingest_state (source, offset) VALUES (?, ?) ' +
      'ON CONFLICT(source) DO UPDATE SET offset = excluded.offset',
  ).run(spinePath, consumed);

  return { inserted, malformed, offset: consumed, unattributed, more };
}

/**
 * Drain the whole backlog, one chunk at a time. Returns the summed result.
 *
 * `maxChunks` bounds a single invocation so a caller can index incrementally
 * rather than blocking for the entire 1.2 GB history; when it stops early it
 * says so via `more`, and never pretends to be finished.
 */
export function indexSpineFully(
  db: Database.Database,
  spinePath: string,
  maxChunks = Number.POSITIVE_INFINITY,
): IndexResult {
  const total: IndexResult = {
    inserted: 0,
    malformed: 0,
    offset: 0,
    unattributed: 0,
    more: false,
  };
  let chunks = 0;
  for (;;) {
    const r = indexSpine(db, spinePath);
    total.inserted += r.inserted;
    total.malformed += r.malformed;
    total.unattributed += r.unattributed;
    total.offset = r.offset;
    total.more = r.more;
    chunks += 1;
    if (!r.more || chunks >= maxChunks) break;
  }
  return total;
}

/**
 * The attribution rate over a window — one number, published rather than asked
 * for. It was ~64% on 2026-08-21; the target is that an unattributed event
 * cannot be written at all.
 */
export function attributionRate(db: Database.Database, since?: string): AttributionRate {
  const rows = (
    since
      ? db.prepare('SELECT role, count(*) AS n FROM events WHERE ts >= ? GROUP BY role').all(since)
      : db.prepare('SELECT role, count(*) AS n FROM events GROUP BY role').all()
  ) as { role: string; n: number }[];

  const byRole: Record<string, number> = {};
  let total = 0;
  let attributed = 0;
  for (const r of rows) {
    byRole[r.role || '<missing>'] = r.n;
    total += r.n;
    if (KNOWN_ROLES.has(r.role)) attributed += r.n;
  }
  // A rate over ZERO events is not 100%. Reporting 1.0 for an empty window would
  // make a dead indexer look perfect — the exact shape this card exists to kill.
  const rate = total === 0 ? 0 : attributed / total;
  return { total, attributed, rate, byRole };
}

/** "What did <role> do between t1 and t2" — the question Jeff actually asks. */
export function actionsFor(
  db: Database.Database,
  role: string,
  from: string,
  to: string,
): { ts: string; event: string; tool: string | null; card_id: string | null }[] {
  return db
    .prepare(
      'SELECT ts, event, tool, card_id FROM events ' +
        'WHERE role = ? AND ts >= ? AND ts <= ? ORDER BY ts',
    )
    .all(role, from, to) as {
    ts: string;
    event: string;
    tool: string | null;
    card_id: string | null;
  }[];
}
