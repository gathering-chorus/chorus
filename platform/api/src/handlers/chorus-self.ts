/**
 * GET /api/chorus/self — Read-only filtered search for Self (DEC-068, extracted #2189).
 *
 * Source whitelist (memory, story, decision, brief, adr) blocks raw sessions
 * (claude) and ops events (spine). Runs FTS + semantic + SPARQL, merges via RRF.
 * FTS falls back to LIKE when MATCH throws (stale index). Semantic is skipped
 * if no semanticSearch fn is provided. SPARQL is best-effort; throws swallow.
 *
 * Dependencies injected — testable without a live DB/lance/fuseki.
 *
 * NOTE (Silas feedback #2189): semanticSearch / sparqlSearch / mergeUnified /
 * emitSearchEvent are injected as typed-fn deps because they're currently
 * private to server.ts. Acceptable while there's only one consumer.
 * If a second handler needs any of these helpers, extract to src/lib/search.ts
 * and have both consumers import directly — do not propagate the injection.
 */
import type Database from 'better-sqlite3';

export type SemanticSearchFn = (query: string, limit: number) => Promise<Array<{ source?: string } & Record<string, unknown>>>;
export type SparqlSearchFn = (query: string, limit: number) => Promise<unknown[]>;
export type MergeUnifiedFn = (
  ftsResults: unknown[],
  semanticResults: unknown[],
  sparqlResults: unknown[],
  limit: number,
) => unknown[];
export type EmitSearchEventFn = (fields: Record<string, string | number>) => void;

export interface SelfDeps {
  db: Database.Database;
  semanticSearch?: SemanticSearchFn;
  sparqlSearch?: SparqlSearchFn;
  mergeUnified: MergeUnifiedFn;
  emitSearchEvent?: EmitSearchEventFn;
  whitelist?: Set<string>;
  now?: () => number;
}

export interface SelfInput {
  q?: string;
  limit?: string;
}

export interface SelfResult {
  status: number;
  body: unknown;
}

const DEFAULT_WHITELIST = new Set(['memory', 'story', 'decision', 'brief', 'adr']);

function runFtsSearch(db: SelfDeps['db'], q: string, whitelist: Set<string>, limit: number): unknown[] {
  const sourceList = Array.from(whitelist).map((s) => `'${s}'`).join(',');
  const ftsQuery = q.replace(/-/g, ' ');
  try {
    return db.prepare(
      `SELECT m.id, m.source, m.channel, m.role, m.content, m.timestamp,
              snippet(messages_fts, 0, '<b>', '</b>', '...', 40) as snippet
       FROM messages_fts f
       JOIN messages m ON f.rowid = m.id
       WHERE messages_fts MATCH ?
       AND m.source IN (${sourceList})
       ORDER BY m.timestamp DESC
       LIMIT ?`,
    ).all(ftsQuery, limit);
  } catch {
    return db.prepare(
      `SELECT id, source, channel, role, content, timestamp, NULL as snippet
       FROM messages
       WHERE content LIKE ?
       AND source IN (${sourceList})
       ORDER BY timestamp DESC
       LIMIT ?`,
    ).all(`%${q}%`, limit);
  }
}

async function runSemanticSearch(semanticSearch: SelfDeps['semanticSearch'], q: string, whitelist: Set<string>, limit: number): Promise<unknown[]> {
  if (!semanticSearch) return [];
  try {
    const rawSem = await semanticSearch(q, limit * 3);
    return rawSem.filter((r) => whitelist.has(r.source || '')).slice(0, limit);
  } catch {
    return [];
  }
}

async function runSparqlSearch(sparqlSearch: SelfDeps['sparqlSearch'], q: string, limit: number): Promise<unknown[]> {
  if (!sparqlSearch) return [];
  try {
    return await sparqlSearch(q, limit);
  } catch {
    return [];
  }
}

export async function fetchSelf(
  {
    db, semanticSearch, sparqlSearch, mergeUnified,
    emitSearchEvent = () => {},
    whitelist = DEFAULT_WHITELIST,
    now = Date.now,
  }: SelfDeps,
  { q, limit: limitRaw }: SelfInput,
): Promise<SelfResult> {
  if (!q) {
    return { status: 400, body: { error: 'Missing required parameter: q' } };
  }
  const limit = Math.min(parseInt(limitRaw || '10', 10) || 10, 50);
  const searchStart = now();

  const ftsResults = runFtsSearch(db, q, whitelist, limit);
  const semResults = await runSemanticSearch(semanticSearch, q, whitelist, limit);
  const sparqlResults = await runSparqlSearch(sparqlSearch, q, limit);
  const merged = mergeUnified(ftsResults, semResults, sparqlResults, limit);

  emitSearchEvent({
    system: 'chorus-self',
    query: q.slice(0, 200),
    mode: 'self',
    result_count: merged.length,
    sources: `fts=${ftsResults.length},semantic=${semResults.length},sparql=${sparqlResults.length}`,
    duration_ms: now() - searchStart,
  });

  return {
    status: 200,
    body: {
      results: merged,
      total: merged.length,
      mode: 'self',
      sources: {
        fts: ftsResults.length,
        semantic: semResults.length,
        sparql: sparqlResults.length,
      },
      filter: { whitelist: Array.from(whitelist) },
    },
  };
}
