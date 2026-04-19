// Embed query helper (extracted from server.ts for #2205).
//
// Wraps Ollama's /api/embeddings endpoint with:
// - exponential-backoff retry (configurable attempts + delays)
// - LRU cache on normalized (lowercase + trimmed) text
// - TTL expiry
// - size-capped with oldest-first eviction
//
// Ollama embed is ~1s per call (#2168 AC-14). Envelope queries repeat
// or have minor variants — caching cuts perceived latency dramatically.
//
// Dependencies are injected so tests run hermetically with no network.

export interface EmbedderDeps {
  ollamaUrl: string;
  model: string;
  fetchFn?: typeof fetch;
  maxRetries?: number;
  backoffMs?: readonly number[];
  cacheMax?: number;
  cacheTtlMs?: number;
  now?: () => number;
  requestTimeoutMs?: number;
}

export type Embedder = (text: string) => Promise<number[]>;

const DEFAULT_MAX_RETRIES = 3;
const DEFAULT_BACKOFF_MS = [1000, 2000, 4000] as const;
const DEFAULT_CACHE_MAX = 128;
const DEFAULT_CACHE_TTL_MS = 10 * 60 * 1000;
const DEFAULT_REQUEST_TIMEOUT_MS = 15000;

interface CacheEntry {
  vec: number[];
  ts: number;
}

export function createEmbedder(deps: EmbedderDeps): Embedder {
  const fetchFn = deps.fetchFn ?? fetch;
  const maxRetries = deps.maxRetries ?? DEFAULT_MAX_RETRIES;
  const backoffMs = deps.backoffMs ?? DEFAULT_BACKOFF_MS;
  const cacheMax = deps.cacheMax ?? DEFAULT_CACHE_MAX;
  const cacheTtlMs = deps.cacheTtlMs ?? DEFAULT_CACHE_TTL_MS;
  const now = deps.now ?? (() => Date.now());
  const requestTimeoutMs = deps.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;

  const cache = new Map<string, CacheEntry>();

  return async function embed(text: string): Promise<number[]> {
    const key = text.trim().toLowerCase();
    const t = now();
    const hit = cache.get(key);
    if (hit && t - hit.ts < cacheTtlMs) {
      // LRU touch — re-insert at newest position.
      cache.delete(key);
      cache.set(key, hit);
      return hit.vec;
    }

    let lastErr: Error | null = null;
    for (let attempt = 0; attempt < maxRetries; attempt++) {
      try {
        const res = await fetchFn(`${deps.ollamaUrl}/api/embeddings`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ model: deps.model, prompt: text }),
          signal: AbortSignal.timeout(requestTimeoutMs),
        });
        if (!res.ok) throw new Error(`Ollama embed failed: ${res.status}`);
        const data = (await res.json()) as { embedding: number[] };
        cache.set(key, { vec: data.embedding, ts: t });
        if (cache.size > cacheMax) {
          const oldest = cache.keys().next().value;
          if (oldest !== undefined) cache.delete(oldest);
        }
        return data.embedding;
      } catch (err) {
        lastErr = err as Error;
        if (attempt < maxRetries - 1 && backoffMs[attempt] > 0) {
          await new Promise(r => setTimeout(r, backoffMs[attempt]));
        }
      }
    }
    throw lastErr || new Error('Ollama embed failed after retries');
  };
}
