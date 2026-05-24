/**
 * GET /api/chorus/search — Unified search across FTS/semantic/SPARQL (extracted #2189).
 *
 * Modes:
 *   fts (default)        — FTS5 MATCH, falls back to LIKE on syntax error
 *   recency              — FTS with timestamp DESC (same as fts)
 *   relevance            — FTS with bm25 ASC
 *   semantic             — lance vector search only
 *   unified              — FTS + semantic + SPARQL via mergeUnified (RRF)
 *   hybrid               — FTS + semantic via mergeRRF
 *
 * Over-fetches by 1 to detect truncation, trims before returning.
 * Every dep injected so tests don't need real db/lance/fuseki/embed.
 *
 * NOTE (Silas feedback #2189): mergeUnified / mergeRRF / semanticSearch /
 * sparqlSearch / emitSearchEvent / buildSearchMeta / enrichHit /
 * resolveSearchLimit are injected as typed-fn deps because they're currently
 * private to server.ts. /chorus/self also consumes this pattern — we already
 * have 2 handlers sharing these. Next time any of these get touched for a
 * real change, extract to src/lib/search.ts and switch both to direct import.
 * The injection is fine for one or two consumers; don't propagate further.
 */
import type Database from 'better-sqlite3';

export type SemanticSearchFn = (
  query: string,
  limit: number,
  role?: string,
) => Promise<Array<{ source?: string } & Record<string, unknown>>>;
export type SparqlSearchFn = (query: string, limit: number) => Promise<unknown[]>;
export type MergeUnifiedFn = (
  fts: unknown[],
  semantic: unknown[],
  sparql: unknown[],
  limit: number,
) => unknown[];
export type MergeRRFFn = (
  fts: unknown[],
  semantic: unknown[],
  limit: number,
  query: string,
) => unknown[];
export type EmitSearchEventFn = (fields: Record<string, string | number>) => void;
export type BuildSearchMetaFn = (
  results: Array<{ timestamp?: string; source?: string; domain?: string }>,
  db?: Database.Database,
) => Record<string, unknown>;
export type EnrichHitFn = (r: unknown, now: number) => unknown;
export type ResolveSearchLimitFn = (raw: string | undefined) => { limit: number; explicit: boolean };

export interface SearchDeps {
  db: Database.Database;
  semanticSearch?: SemanticSearchFn;
  sparqlSearch: SparqlSearchFn;
  mergeUnified: MergeUnifiedFn;
  mergeRRF: MergeRRFFn;
  emitSearchEvent?: EmitSearchEventFn;
  buildSearchMeta: BuildSearchMetaFn;
  enrichHit: EnrichHitFn;
  resolveSearchLimit: ResolveSearchLimitFn;
  now?: () => number;
}

export interface SearchInput {
  q?: string;
  limit?: string;
  role?: string;
  mode?: string;
}

export interface SearchResult {
  status: number;
  body: unknown;
}

/**
 * #3051 — turn arbitrary user text into a valid FTS5 MATCH expression.
 * Replaces every run of non-word chars with a space, then wraps each token in
 * double quotes (an FTS5 literal). Guarantees MATCH never throws on operators,
 * parens, colons, or quotes — so /search never drops to the synchronous
 * `content LIKE '%q%'` full-table scan over 1.26M rows that froze the spine.
 * Returns '' when the input has no word tokens (caller then returns no rows).
 */
export function toFtsMatchQuery(raw: string): string {
  return raw
    .replace(/[^\p{L}\p{N}_]+/gu, ' ')
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => `"${t}"`)
    .join(' ');
}

function runFtsQuery(db: SearchDeps['db'], q: string, fetchLimit: number, role: string | undefined, mode: string): unknown[] {
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

type FormatFn = (raw: unknown[], mode: string, includeDb: boolean, extra?: Record<string, unknown>) => unknown;
type EmitFn = (mode: string, count: number, extra?: Record<string, unknown>) => void;

async function trySemanticMode(
  deps: SearchDeps, q: string, fetchLimit: number, role: string | undefined,
  limit: number, limitExplicit: boolean, format: FormatFn, emit: EmitFn,
): Promise<SearchResult> {
  const { semanticSearch, buildSearchMeta } = deps;
  if (!semanticSearch) {
    return { status: 200, body: {
      results: [], total: 0, mode: 'semantic', error: 'Semantic index not available',
      _meta: { ...buildSearchMeta([]), limit_applied: limit, limit_default: !limitExplicit, truncated: false },
    } };
  }
  try {
    const results = await semanticSearch(q, fetchLimit, role);
    emit('semantic', results.length);
    return { status: 200, body: format(results, 'semantic', false) };
  } catch (err) {
    return { status: 500, body: { error: `Semantic search failed: ${err}` } };
  }
}

async function tryUnifiedMode(
  deps: SearchDeps, q: string, fetchLimit: number, role: string | undefined,
  ftsResults: unknown[], format: FormatFn, emit: EmitFn,
): Promise<SearchResult | null> {
  const { semanticSearch, sparqlSearch, mergeUnified } = deps;
  try {
    const [semResults, sparqlResults] = await Promise.all([
      semanticSearch ? semanticSearch(q, fetchLimit, role) : Promise.resolve([] as unknown[]),
      sparqlSearch(q, fetchLimit),
    ]);
    const merged = mergeUnified(ftsResults, semResults, sparqlResults, fetchLimit);
    emit('unified', merged.length, { sources: `fts=${ftsResults.length},semantic=${semResults.length},sparql=${sparqlResults.length}` });
    return { status: 200, body: format(merged, 'unified', true, {
      sources: { fts: ftsResults.length, semantic: semResults.length, sparql: sparqlResults.length },
    }) };
  } catch { return null; }
}

async function tryHybridMode(
  deps: SearchDeps, q: string, fetchLimit: number, role: string | undefined,
  ftsResults: unknown[], format: FormatFn, emit: EmitFn,
): Promise<SearchResult | null> {
  const { semanticSearch, mergeRRF } = deps;
  if (!semanticSearch) return null;
  try {
    const semResults = await semanticSearch(q, fetchLimit, role);
    const merged = mergeRRF(ftsResults, semResults, fetchLimit, q);
    emit('hybrid', merged.length);
    return { status: 200, body: format(merged, 'hybrid', true) };
  } catch { return null; }
}

export async function fetchSearch(
  deps: SearchDeps,
  { q, limit: limitRaw, role, mode: modeRaw }: SearchInput,
): Promise<SearchResult> {
  if (!q) return { status: 400, body: { error: 'Missing required parameter: q' } };

  const { db, emitSearchEvent = () => {}, buildSearchMeta, enrichHit, resolveSearchLimit, now = Date.now } = deps;
  const { limit, explicit: limitExplicit } = resolveSearchLimit(limitRaw);
  const mode = (modeRaw || 'fts').toLowerCase();
  const searchStart = now();
  const fetchLimit = limit + 1;

  const format: FormatFn = (rawResults, resolvedMode, includeDb, extra = {}) => {
    const truncated = rawResults.length > limit;
    const enriched = rawResults.slice(0, limit).map((r) => enrichHit(r, now())) as Array<{ timestamp?: string; source?: string; domain?: string }>;
    const meta = {
      ...buildSearchMeta(enriched, includeDb ? db : undefined),
      limit_applied: limit, limit_default: !limitExplicit, truncated,
    };
    return { results: enriched, total: enriched.length, mode: resolvedMode, _meta: meta, ...extra };
  };

  let ftsMs = 0; // #3051 AC4: per-request FTS time, so the next stall is attributable
  const emit: EmitFn = (m, count, extra = {}) => emitSearchEvent({
    system: 'chorus-api', query: q.slice(0, 200), mode: m,
    result_count: count, duration_ms: now() - searchStart, fts_ms: ftsMs,
    ...(role ? { role_filter: role } : {}), ...extra,
  });

  if (mode === 'semantic') {
    return trySemanticMode(deps, q, fetchLimit, role, limit, limitExplicit, format, emit);
  }

  const ftsStart = now();
  const ftsResults = runFtsQuery(db, q, fetchLimit, role, mode);
  ftsMs = now() - ftsStart;

  if (mode === 'unified') {
    const result = await tryUnifiedMode(deps, q, fetchLimit, role, ftsResults, format, emit);
    if (result) return result;
  }

  if (mode === 'hybrid') {
    const result = await tryHybridMode(deps, q, fetchLimit, role, ftsResults, format, emit);
    if (result) return result;
  }

  const resolvedMode = mode === 'recency' || mode === 'relevance' ? mode : 'fts';
  emit(resolvedMode, ftsResults.length);
  return { status: 200, body: format(ftsResults, resolvedMode, true) };
}
