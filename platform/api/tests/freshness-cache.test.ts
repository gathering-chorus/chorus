// @test-type: unit — compute and clock are injected; no service, no store.
// #3060 - /api/chorus/freshness recomputes a COUNT over the index. In May it was
// ~1.4s on the main thread; by 2026-10-07 it was 11-15s and froze chorus-api.
//
// The cache serves the last snapshot at once and recomputes in the background,
// and compute is async (server.ts runs it in a worker thread), so no request
// waits on a recompute once a snapshot exists.

import { createFreshnessCache } from '../src/freshness-cache';

describe('createFreshnessCache (#3060 - freshness never blocks a request)', () => {
  const snap = (n: number) => ({ status: 200, body: { tag: n } }) as any;

  // A compute the test resolves by hand, so "still recomputing" is a real state.
  function deferredCompute() {
    const resolvers: Array<(v: unknown) => void> = [];
    let calls = 0;
    const compute = () => {
      calls++;
      const n = calls;
      return new Promise<any>((resolve) => resolvers.push(() => resolve(snap(n))));
    };
    return { compute, resolvers, calls: () => calls };
  }

  it('computes once on first get, then serves cached within TTL without recomputing', async () => {
    let t = 1000;
    const d = deferredCompute();
    const cache = createFreshnessCache(d.compute, { ttlMs: 100, now: () => t });

    const first = cache.get(); // cold: waits for the one recompute
    d.resolvers[0](undefined);
    expect(await first).toEqual(snap(1));

    t = 1050; // within TTL
    expect(await cache.get()).toEqual(snap(1));
    expect(d.calls()).toBe(1);
  });

  it('after TTL: returns the STALE snapshot while the recompute is still running', async () => {
    let t = 1000;
    const d = deferredCompute();
    const cache = createFreshnessCache(d.compute, { ttlMs: 100, now: () => t });

    const first = cache.get();
    d.resolvers[0](undefined);
    await first;

    t = 2000; // past TTL; the recompute starts and is left unresolved
    expect(await cache.get()).toEqual(snap(1)); // answered without waiting on it
    expect(d.calls()).toBe(2);

    d.resolvers[1](undefined); // the background recompute finishes
    await new Promise((r) => setImmediate(r));
    expect(await cache.get()).toEqual(snap(2));
  });

  it('does not stampede: many stale gets start only one recompute', async () => {
    let t = 1000;
    const d = deferredCompute();
    const cache = createFreshnessCache(d.compute, { ttlMs: 100, now: () => t });

    const first = cache.get();
    d.resolvers[0](undefined);
    await first;

    t = 2000;
    await Promise.all([cache.get(), cache.get(), cache.get()]);
    expect(d.calls()).toBe(2); // one cold + ONE background, not three
  });

  it('a failed background recompute keeps the old snapshot and the next stale get retries', async () => {
    let t = 1000;
    let fail = false;
    let calls = 0;
    const cache = createFreshnessCache(
      () => { calls++; return fail ? Promise.reject(new Error('index locked')) : Promise.resolve(snap(calls)); },
      { ttlMs: 100, now: () => t },
    );
    expect(await cache.get()).toEqual(snap(1));

    t = 2000;
    fail = true;
    expect(await cache.get()).toEqual(snap(1));
    await new Promise((r) => setImmediate(r));
    expect(calls).toBe(2);

    fail = false;
    expect(await cache.get()).toEqual(snap(1)); // still stale: starts a retry
    await new Promise((r) => setImmediate(r));
    expect(calls).toBe(3);
    expect(await cache.get()).toEqual(snap(3));
  });
});
