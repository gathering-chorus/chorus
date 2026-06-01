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
  // #3147 (Wren · search relevance) — AUTHORITY weight. Plain bm25 lets dense session
  // chatter outrank authoritative docs/decisions/memory (proven: "heidegger" returned 5×
  // "check the research" chatter, burying the Versammlung research doc + DEC-031). Penalize
  // chatter sources so knowledge surfaces. Verified on the live index before landing.
  // #3171 (Wren) — the context-inject queries mode=hybrid (context_inject.rs build_search_url).
  // Pre-#3171, hybrid fell through to `timestamp DESC` (recency), so the inject surfaced recent
  // session chatter, NOT knowledge — the #3147 authority fix was relevance-only and never reached
  // the inject (proven: live inject candidates were jeff/wren session messages; unit test red).
  // Extend the authority weight to hybrid so its FTS half ranks knowledge over chatter; hybrid
  // still blends semantic via RRF at the merge layer. recency stays pure-recency (rebuild path).
  const authorityOrder =
    "bm25(messages_fts) + CASE m.source WHEN 'claude' THEN 8.0 WHEN 'clearing' THEN 8.0 WHEN 'slack' THEN 4.0 ELSE 0 END ASC";
  const ftsOrderBy = mode === 'relevance' || mode === 'hybrid'
    ? authorityOrder
    : 'm.timestamp DESC';
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
