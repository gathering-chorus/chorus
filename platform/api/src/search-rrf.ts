/**
 * #2168 AC-14 — query-aware RRF weighting for hybrid search.
 *
 * Default: semantic wins (captures intent).
 * FTS-boost: when the query carries exact tokens (card IDs like #2168,
 * dotted filenames like server.ts, slash-paths like platform/api,
 * underscored symbols like query_chorus_hybrid), FTS outranks semantic
 * for that query.
 */

export interface SemanticResult {
  msg_id: number;
  source: string;
  channel: string;
  role: string;
  content: string;
  timestamp: string;
  score: number;
}

/** FTS row shape used by mergeRRF — downstream consumers may have more fields. */
export interface FtsResult {
  id?: number;
  msg_id?: number;
  source?: string;
  channel?: string;
  role?: string;
  content?: string;
  timestamp?: string;
  snippet?: string | null;
  _semantic_score?: number;
}

/** Merged RRF entry — union of FTS + semantic fields with RRF score. */
export interface RrfMerged {
  id?: number;
  msg_id?: number;
  source?: string;
  channel?: string;
  role?: string;
  content?: string;
  timestamp?: string;
  snippet?: string | null;
  _semantic_score?: number;
  _rrf_score: number;
}

export function hasExactToken(query: string): boolean {
  return /#\d+|\b[a-z][a-z0-9_]+\.(ts|rs|sh|md|py|json|toml|bats|html)\b|[/][a-z][a-z0-9_-]*[/]|[a-z]+_[a-z0-9_]+/i.test(query);
}

export function mergeRRF(
  ftsResults: FtsResult[],
  semResults: SemanticResult[],
  limit: number,
  query = '',
  k = 60,
): RrfMerged[] {
  const scoreMap = new Map<number, { score: number; result: FtsResult }>();
  const exactToken = hasExactToken(query);
  const ftsWeight = exactToken ? 2.0 : 1.0;
  const semanticWeight = exactToken ? 1.0 : 2.0;

  ftsResults.forEach((r, i) => {
    const key = r.id ?? r.msg_id;
    if (key === undefined) return;
    const rrfScore = ftsWeight / (k + i + 1);
    scoreMap.set(key, { score: rrfScore, result: r });
  });

  semResults.forEach((r, i) => {
    const key = r.msg_id;
    const rrfScore = semanticWeight / (k + i + 1);
    const existing = scoreMap.get(key);
    if (existing) {
      existing.score += rrfScore;
    } else {
      scoreMap.set(key, {
        score: rrfScore,
        result: {
          source: r.source,
          channel: r.channel,
          role: r.role,
          content: r.content,
          timestamp: r.timestamp,
          snippet: null,
          _semantic_score: r.score,
        },
      });
    }
  });

  return Array.from(scoreMap.values())
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((e) => ({ ...e.result, _rrf_score: e.score }));
}
