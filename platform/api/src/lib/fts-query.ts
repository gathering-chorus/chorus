/**
 * Shared FTS query (#3086) — the single source of the search SQL so the
 * in-process path and the worker_threads offload run BYTE-IDENTICAL queries.
 * Parity is by construction: both import runFtsQueryOnDb from here.
 *
 * Extracted verbatim from chorus-search.ts (was runFtsQuery + toFtsMatchQuery).
 * No behavior change — the existing search tests are the regression net.
 */
import type Database from 'better-sqlite3';

/** Sanitize a raw query into a quoted FTS5 MATCH string (word tokens only). */
export function toFtsMatchQuery(raw: string): string {
  return raw
    .replace(/[^\p{L}\p{N}_]+/gu, ' ')
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => `"${t}"`)
    .join(' ');
}

/**
 * Run the FTS5 query against a better-sqlite3 connection. Synchronous (better-sqlite3
 * is sync) — on the 1.1GB index this is the multi-second event-loop blocker (#3079),
 * which is why #3086 calls this from a worker thread instead of the serving loop.
 */
export function runFtsQueryOnDb(
  db: Database.Database,
  q: string,
  fetchLimit: number,
  role: string | undefined,
  mode: string,
): unknown[] {
  const ftsQuery = toFtsMatchQuery(q);
  if (!ftsQuery) return []; // no word tokens — nothing to match, never scan
  const ftsOrderBy = mode === 'relevance' ? 'bm25(messages_fts) ASC' : 'm.timestamp DESC';
  const params: unknown[] = role ? [ftsQuery, role, fetchLimit] : [ftsQuery, fetchLimit];
  const roleFilter = role ? 'AND m.role = ?' : '';
  try {
    return db.prepare(
      `SELECT m.id, m.source, m.channel, m.role, m.author, m.content, m.timestamp,
              snippet(messages_fts, 0, '<b>', '</b>', '...', 40) as snippet
       FROM messages_fts f
       JOIN messages m ON f.rowid = m.id
       WHERE messages_fts MATCH ?
       ${roleFilter}
       ORDER BY ${ftsOrderBy}
       LIMIT ?`,
    ).all(...params);
  } catch {
    // #3051: do NOT fall back to `content LIKE '%q%'` — that is an unindexed full
    // scan over 1.26M rows (~3.6s synchronous, freezes the whole spine). Sanitized
    // FTS shouldn't throw; if it ever does, return empty rather than scan.
    return [];
  }
}
