// #3060 - stale-while-revalidate cache for GET /api/chorus/freshness.
//
// fetchFreshness does ~1.4s of synchronous work (read+count the 170MB spine log,
// COUNT spine rows). Doing that per request blocks the chorus-api event loop -
// the coordination spine - freezing search / context-inject / cards / nudge.
//
// This cache decouples the cost from request rate: the request path returns the
// last good snapshot in <1ms; when the snapshot ages past ttlMs the next get()
// still returns the (stale) snapshot immediately and schedules a single
// background recompute off the request tick. No single request blocks >100ms.
//
// Pure + injectable (now, schedule, compute) so it is fully unit-testable. The
// residual synchronous cost of the recompute itself moving off the loop entirely
// (worker thread) is the structural follow-on tracked in #3055.

export interface FreshnessCacheOpts {
  ttlMs: number;
  now?: () => number;
  // schedule a background recompute off the current tick (default setImmediate).
  schedule?: (fn: () => void) => void;
}

export interface FreshnessCache<T> {
  get(): T;
}

export function createFreshnessCache<T>(
  compute: () => T,
  opts: FreshnessCacheOpts,
): FreshnessCache<T> {
  const now = opts.now ?? Date.now;
  const schedule = opts.schedule ?? ((fn: () => void) => setImmediate(fn));
  const ttlMs = opts.ttlMs;

  let snapshot: T | undefined;
  let computedAt = 0;
  let refreshing = false;

  function refresh(): void {
    try {
      snapshot = compute();
      computedAt = now();
    } finally {
      refreshing = false;
    }
  }

  return {
    get(): T {
      // Cold start: nothing cached yet — compute once synchronously so the very
      // first caller still gets a real answer. Pre-warm at boot to avoid paying
      // this on a live request.
      if (snapshot === undefined) {
        refresh();
        return snapshot as T;
      }

      // Stale: serve the existing snapshot immediately, schedule one refresh.
      if (now() - computedAt >= ttlMs && !refreshing) {
        refreshing = true;
        schedule(refresh);
      }

      return snapshot;
    },
  };
}
