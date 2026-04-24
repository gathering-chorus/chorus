// Search fusion helpers (extracted from server.ts for #2205 wave 3).
//
// Three pure functions:
// - mergeUnified: Reciprocal Rank Fusion across FTS + semantic + SPARQL hits.
// - resolveSearchLimit: query-string parsing with default/cap semantics (#2174).
// - enrichHit: per-result freshness_s computation (#2174 AC-1/2).

export interface SemanticResult {
  msg_id: number;
  source: string;
  channel: string;
  role: string;
  content: string;
  timestamp: string;
  score: number;
}

export interface SparqlResult {
  uri: string;
  type: string;
  domain: string;
  label: string;
  content: string;
  score: number;
}

export interface UnifiedResult {
  id?: number;
  uri?: string;
  source: string;
  type?: string;
  domain?: string;
  role?: string;
  content: string;
  timestamp?: string;
  label?: string;
  _rrf_score: number;
  _sources: string[];
}

/** Full-text search result row — caller downcasts the raw SQL row into this shape. */
export interface FtsResult {
  id: number;
  source?: string;
  role?: string;
  content: string;
  timestamp?: string;
}

/**
 * Reciprocal Rank Fusion: each source contributes 1/(k+rank+1) to an item's
 * score, keyed so FTS and semantic hits on the same msg_id combine, while
 * SPARQL hits stay under a distinct uri: key.
 */
export function mergeUnified(
  ftsResults: FtsResult[],
  semanticResults: SemanticResult[],
  sparqlResults: SparqlResult[],
  limit: number,
  k: number = 60,
): UnifiedResult[] {
  const scoreMap = new Map<string, UnifiedResult>();

  ftsResults.forEach((r, i) => {
    const key = `chorus:${r.id}`;
    const entry = scoreMap.get(key) || {
      id: r.id, source: r.source || 'chorus', role: r.role, content: r.content,
      timestamp: r.timestamp, _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('fts')) entry._sources.push('fts');
    scoreMap.set(key, entry);
  });

  semanticResults.forEach((r, i) => {
    const key = `chorus:${r.msg_id}`;
    const entry = scoreMap.get(key) || {
      id: r.msg_id, source: r.source || 'chorus', role: r.role, content: r.content,
      timestamp: r.timestamp, _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('semantic')) entry._sources.push('semantic');
    scoreMap.set(key, entry);
  });

  sparqlResults.forEach((r, i) => {
    const key = `sparql:${r.uri}`;
    const entry = scoreMap.get(key) || {
      uri: r.uri, source: 'sparql', type: r.type, domain: r.domain,
      label: r.label, content: r.content,
      _rrf_score: 0, _sources: [] as string[],
    };
    entry._rrf_score += 1 / (k + i + 1);
    if (!entry._sources.includes('sparql')) entry._sources.push('sparql');
    scoreMap.set(key, entry);
  });

  return Array.from(scoreMap.values())
    .sort((a, b) => b._rrf_score - a._rrf_score)
    .slice(0, limit);
}

// Token-economy defaults (#2174 AC-6).
export const SEARCH_DEFAULT_LIMIT = 5;
export const SEARCH_MAX_LIMIT = 100;

export function resolveSearchLimit(raw: string | undefined): { limit: number; explicit: boolean } {
  if (raw === undefined) return { limit: SEARCH_DEFAULT_LIMIT, explicit: false };
  const n = parseInt(raw, 10);
  if (isNaN(n) || n < 1) return { limit: SEARCH_DEFAULT_LIMIT, explicit: false };
  return { limit: Math.min(n, SEARCH_MAX_LIMIT), explicit: true };
}

// #2174 AC-1: per-hit freshness. Structured so agents don't re-parse content
// for timestamp (AC-2 semantic).
export function enrichHit(r: unknown, now: number): unknown {
  const obj = (r && typeof r === 'object') ? r as { timestamp?: string } : {};
  const ts = obj.timestamp;
  let freshness_s = 0;
  if (ts) {
    const t = new Date(ts).getTime();
    if (!isNaN(t)) freshness_s = Math.max(0, Math.round((now - t) / 1000));
  }
  return { ...(r as object), freshness_s };
}
