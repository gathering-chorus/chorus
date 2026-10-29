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

export interface FtsFilterOptions {
  order?: string;
  before?: string;
  after?: string;
  offset?: number;
}

/**
 * Run the FTS5 query against a better-sqlite3 connection. Synchronous (better-sqlite3
 * is sync) — on the 1.1GB index this is the multi-second event-loop blocker (#3079),
 * which is why #3086 calls this from a worker thread instead of the serving loop.
 */
function runChannelRecentQuery(
  db: Database.Database,
  channel: string,
  fetchLimit: number,
  role: string | undefined,
  opts?: FtsFilterOptions,
): unknown[] {
  const sessionRole = channel.startsWith('session:') ? channel.slice('session:'.length) : '';
  const directParams: unknown[] = [channel, sessionRole];
  const clauses: string[] = [];
  if (role) {
    clauses.push('AND m.role = ?');
    directParams.push(role);
  }
  if (opts?.before) {
    clauses.push('AND m.timestamp < ?');
    directParams.push(opts.before);
  }
  if (opts?.after) {
    clauses.push('AND m.timestamp > ?');
    directParams.push(opts.after);
  }
  directParams.push(fetchLimit);
  const offsetVal = opts?.offset && opts.offset > 0 ? opts.offset : 0;
  if (offsetVal > 0) directParams.push(offsetVal);
  const orderDir = opts?.order === 'oldest' ? 'ASC' : 'DESC';
  const limitClause = offsetVal > 0 ? 'LIMIT ? OFFSET ?' : 'LIMIT ?';
  try {
    return db.prepare(
      `SELECT m.id, m.source, m.channel, m.role, m.author, m.content, m.timestamp,
              substr(m.content, 1, 200) as snippet
       FROM messages m
       WHERE (m.channel = ? OR (m.channel IS NULL AND m.role = ?))
       ${clauses.join('\n       ')}
       ORDER BY m.timestamp ${orderDir}
       ${limitClause}`,
    ).all(...directParams);
  } catch {
    return [];
  }
}

function buildFtsSqlAndParams(
  ftsQuery: string,
  fetchLimit: number,
  role: string | undefined,
  mode: string,
  channel: string | undefined,
  opts?: FtsFilterOptions,
): { sql: string; params: unknown[] } {
  const authorityOrder =
    "bm25(messages_fts) + CASE m.source WHEN 'claude' THEN 8.0 WHEN 'clearing' THEN 8.0 WHEN 'slack' THEN 4.0 ELSE 0 END ASC";
  const ftsOrderBy =
    opts?.order === 'oldest'
      ? 'm.timestamp ASC'
      : mode === 'relevance' || mode === 'hybrid'
        ? authorityOrder
        : 'm.timestamp DESC';
  const params: unknown[] = [ftsQuery];
  const clauses: string[] = [];
  if (role) {
    clauses.push('AND m.role = ?');
    params.push(role);
  }
  if (channel) {
    clauses.push('AND m.channel = ?');
    params.push(channel);
  }
  if (opts?.before) {
    clauses.push('AND m.timestamp < ?');
    params.push(opts.before);
  }
  if (opts?.after) {
    clauses.push('AND m.timestamp > ?');
    params.push(opts.after);
  }
  params.push(fetchLimit);
  const offsetVal = opts?.offset && opts.offset > 0 ? opts.offset : 0;
  if (offsetVal > 0) params.push(offsetVal);
  const limitClause = offsetVal > 0 ? 'LIMIT ? OFFSET ?' : 'LIMIT ?';
  const sql = `SELECT m.id, m.source, m.channel, m.role, m.author, m.content, m.timestamp,
              snippet(messages_fts, 0, '<b>', '</b>', '...', 40) as snippet
       FROM messages_fts f
       JOIN messages m ON f.rowid = m.id
       WHERE messages_fts MATCH ?
       ${clauses.join('\n       ')}
       ORDER BY ${ftsOrderBy}
       ${limitClause}`;
  return { sql, params };
}

export function runFtsQueryOnDb(
  db: Database.Database,
  q: string,
  fetchLimit: number,
  role: string | undefined,
  mode: string,
  channel?: string,
  opts?: FtsFilterOptions,
): unknown[] {
  const ftsQuery = toFtsMatchQuery(q);
  if (!ftsQuery) {
    return channel ? runChannelRecentQuery(db, channel, fetchLimit, role, opts) : [];
  }
  const { sql, params } = buildFtsSqlAndParams(ftsQuery, fetchLimit, role, mode, channel, opts);
  try {
    return db.prepare(sql).all(...params);
  } catch {
    return [];
  }
}
