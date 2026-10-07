// #3060 - stale-while-revalidate cache for GET /api/chorus/freshness.
//
// The freshness recompute counts rows in the index. In May that was ~1.4s; by
// 2026-10-07 it was 11-15s and froze chorus-api every 30s, because the May cache
// still ran it on the main thread. compute now returns a Promise: server.ts runs
// it in the freshness worker thread (freshness-worker.ts), so the main thread
// only awaits.
//
// The request path returns the last good snapshot at once; when it ages past
// ttlMs the next get() still returns it and starts one background recompute.
// Only the very first get (no snapshot yet) waits, and it waits without blocking.
//
// Pure + injectable (now, compute) so it is fully unit-testable.

export interface FreshnessCacheOpts {
  ttlMs: number;
  now?: () => number;
}

export interface FreshnessCache<T> {
  get(): Promise<T>;
}

export function createFreshnessCache<T>(
  compute: () => Promise<T>,
  opts: FreshnessCacheOpts,
): FreshnessCache<T> {
  const now = opts.now ?? Date.now;
  const ttlMs = opts.ttlMs;

  let snapshot: T | undefined;
  let computedAt = 0;
  let inFlight: Promise<T> | null = null;

  function refresh(): Promise<T> {
    if (!inFlight) {
      inFlight = compute()
        .then((v) => {
          snapshot = v;
          computedAt = now();
          return v;
        })
        .finally(() => {
          inFlight = null;
        });
    }
    return inFlight;
  }

  return {
    get(): Promise<T> {
      // Cold start: nothing cached yet, so the first caller waits for one recompute.
      if (snapshot === undefined) return refresh();

      // Stale: serve the existing snapshot now and start one background recompute.
      // A failed background recompute keeps the old snapshot; the next stale get retries.
      if (now() - computedAt >= ttlMs) refresh().catch(() => undefined);

      return Promise.resolve(snapshot);
    },
  };
}
