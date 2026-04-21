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
  results: unknown[],
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

function runFtsQuery(db: SearchDeps['db'], q: string, fetchLimit: number, role: string | undefined, mode: string): unknown[] {
  const ftsQuery = q.replace(/-/g, ' ');
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
    const likeParams: unknown[] = role ? [`%${q}%`, role, fetchLimit] : [`%${q}%`, fetchLimit];
    const likeRoleFilter = role ? 'AND role = ?' : '';
    return db.prepare(
      `SELECT id, source, channel, role, author, content, timestamp, NULL as snippet
       FROM messages
       WHERE content LIKE ?
       ${likeRoleFilter}
       ORDER BY timestamp DESC
       LIMIT ?`,
    ).all(...likeParams);
  }
}

export async function fetchSearch(
  deps: SearchDeps,
  { q, limit: limitRaw, role, mode: modeRaw }: SearchInput,
): Promise<SearchResult> {
  if (!q) return { status: 400, body: { error: 'Missing required parameter: q' } };

  const { db, semanticSearch, sparqlSearch, mergeUnified, mergeRRF,
    emitSearchEvent = () => {}, buildSearchMeta, enrichHit,
    resolveSearchLimit, now = Date.now } = deps;
  const { limit, explicit: limitExplicit } = resolveSearchLimit(limitRaw);
  const mode = (modeRaw || 'fts').toLowerCase();
  const searchStart = now();
  const fetchLimit = limit + 1;

  const formatResponse = (
    rawResults: unknown[], resolvedMode: string, includeDb: boolean,
    extra: Record<string, unknown> = {},
  ): unknown => {
    const truncated = rawResults.length > limit;
    const enriched = rawResults.slice(0, limit).map((r) => enrichHit(r, now()));
    const meta = {
      ...buildSearchMeta(enriched, includeDb ? db : undefined),
      limit_applied: limit, limit_default: !limitExplicit, truncated,
    };
    return { results: enriched, total: enriched.length, mode: resolvedMode, _meta: meta, ...extra };
  };

  const emit = (m: string, count: number, extra: Record<string, unknown> = {}) => emitSearchEvent({
    system: 'chorus-api', query: q.slice(0, 200), mode: m,
    result_count: count, duration_ms: now() - searchStart,
    ...(role ? { role_filter: role } : {}), ...extra,
  });

  // Semantic-only mode
  if (mode === 'semantic') {
    if (!semanticSearch) {
      return { status: 200, body: {
        results: [], total: 0, mode: 'semantic', error: 'Semantic index not available',
        _meta: { ...buildSearchMeta([]), limit_applied: limit, limit_default: !limitExplicit, truncated: false },
      } };
    }
    try {
      const results = await semanticSearch(q, fetchLimit, role);
      emit('semantic', results.length);
      return { status: 200, body: formatResponse(results, 'semantic', false) };
    } catch (err) {
      return { status: 500, body: { error: `Semantic search failed: ${err}` } };
    }
  }

  const ftsResults = runFtsQuery(db, q, fetchLimit, role, mode);

  // Unified: FTS + semantic + SPARQL
  if (mode === 'unified') {
    try {
      const [semResults, sparqlResults] = await Promise.all([
        semanticSearch ? semanticSearch(q, fetchLimit, role) : Promise.resolve([] as unknown[]),
        sparqlSearch(q, fetchLimit),
      ]);
      const merged = mergeUnified(ftsResults, semResults, sparqlResults, fetchLimit);
      emit('unified', merged.length, { sources: `fts=${ftsResults.length},semantic=${semResults.length},sparql=${sparqlResults.length}` });
      return { status: 200, body: formatResponse(merged, 'unified', true, {
        sources: { fts: ftsResults.length, semantic: semResults.length, sparql: sparqlResults.length },
      }) };
    } catch { /* fall through to FTS-only */ }
  }

  // Hybrid: FTS + semantic
  if (mode === 'hybrid' && semanticSearch) {
    try {
      const semResults = await semanticSearch(q, fetchLimit, role);
      const merged = mergeRRF(ftsResults, semResults, fetchLimit, q);
      emit('hybrid', merged.length);
      return { status: 200, body: formatResponse(merged, 'hybrid', true) };
    } catch { /* fall through to FTS-only */ }
  }

  const resolvedMode = mode === 'recency' || mode === 'relevance' ? mode : 'fts';
  emit(resolvedMode, ftsResults.length);
  return { status: 200, body: formatResponse(ftsResults, resolvedMode, true) };
}
