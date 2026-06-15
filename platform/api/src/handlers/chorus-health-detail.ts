/**
 * GET /api/chorus/health/detail — Deep health snapshot (#1978, extracted #2189).
 *
 * Returns the cached health snapshot augmented with a live Ollama check.
 * Ollama availability is classified as 'up' (2xx), 'degraded' (non-2xx), or
 * 'down' (fetch throws / times out).
 */

export interface HealthCacheSnapshot {
  dbStatus: string;
  dbRows: number;
  vectors: unknown;
  unembedded: unknown;
  hooksStatus: string;
}

export type FetchFn = (
  url: string,
  init?: { signal?: AbortSignal },
) => Promise<{ ok: boolean; status: number }>;

export interface HealthDetailDeps {
  fetchFn?: FetchFn;
  healthCache: HealthCacheSnapshot;
  ollamaUrl: string;
  timestamp: () => string;
}

export interface HealthDetailResult {
  status: number;
  body: {
    db: { status: string; rows: number };
    vectors: unknown;
    unembedded: unknown;
    hooks: { status: string };
    ollama: { status: 'up' | 'degraded' | 'down' | 'unknown' };
    timestamp: string;
  };
}

export async function fetchHealthDetail({
  fetchFn = globalThis.fetch as FetchFn,
  healthCache,
  ollamaUrl,
  timestamp,
}: HealthDetailDeps): Promise<HealthDetailResult> {
  let ollamaStatus: 'up' | 'degraded' | 'down' | 'unknown';
  try {
    const res = await fetchFn(`${ollamaUrl}/api/tags`, { signal: AbortSignal.timeout(3000) });
    ollamaStatus = res.ok ? 'up' : 'degraded';
  } catch {
    ollamaStatus = 'down';
  }

  return {
    status: 200,
    body: {
      db: { status: healthCache.dbStatus, rows: healthCache.dbRows },
      vectors: healthCache.vectors,
      unembedded: healthCache.unembedded,
      hooks: { status: healthCache.hooksStatus },
      ollama: { status: ollamaStatus },
      timestamp: timestamp(),
    },
  };
}
