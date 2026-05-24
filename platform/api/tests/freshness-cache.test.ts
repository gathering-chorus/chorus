// #3060 - /api/chorus/freshness was doing ~1.4s of SYNCHRONOUS work per call
// (read whole 170MB spine log + split, plus COUNT(messages WHERE source=spine))
// on the chorus-api event loop - the coordination spine. Proven by #3058
// timestamps: the 15:46:51.414Z block mapped to GET /api/chorus/freshness 1447ms.
//
// Fix = stale-while-revalidate cache: the request path returns a cached snapshot
// in <1ms; the expensive recompute runs off the request tick (scheduled), so no
// single request blocks the loop >100ms.

import { createFreshnessCache } from '../src/freshness-cache';

describe('createFreshnessCache (#3060 - bound freshness off the loop)', () => {
  const snap = (n: number) => ({ status: 200, body: { tag: n } }) as any;

  it('computes once on first get, then serves cached within TTL without recomputing', () => {
    let calls = 0;
    let t = 1000;
    const cache = createFreshnessCache(() => { calls++; return snap(calls); }, {
      ttlMs: 100,
      now: () => t,
      schedule: () => { /* not used within TTL */ },
    });

    expect(cache.get()).toEqual(snap(1)); // cold: computes synchronously once
    expect(calls).toBe(1);

    t = 1050; // still within TTL
    expect(cache.get()).toEqual(snap(1)); // served from cache
    expect(calls).toBe(1); // NOT recomputed
  });

  it('after TTL: returns STALE snapshot immediately and schedules a background refresh (does not block)', () => {
    let calls = 0;
    let t = 1000;
    const scheduled: Array<() => void> = [];
    const cache = createFreshnessCache(() => { calls++; return snap(calls); }, {
      ttlMs: 100,
      now: () => t,
      schedule: (fn) => { scheduled.push(fn); },
    });

    expect(cache.get()).toEqual(snap(1)); // cold compute -> snap(1)
    t = 2000; // past TTL

    // stale-while-revalidate: returns the OLD snapshot synchronously, no recompute on this tick
    expect(cache.get()).toEqual(snap(1));
    expect(calls).toBe(1);
    expect(scheduled).toHaveLength(1); // a refresh was scheduled off-tick

    scheduled[0](); // run the background refresh
    expect(calls).toBe(2);
    expect(cache.get()).toEqual(snap(2)); // now serves the refreshed snapshot
  });

  it('does not stampede: multiple stale gets schedule only one refresh', () => {
    let t = 1000;
    const scheduled: Array<() => void> = [];
    let calls = 0;
    const cache = createFreshnessCache(() => { calls++; return snap(calls); }, {
      ttlMs: 100,
      now: () => t,
      schedule: (fn) => { scheduled.push(fn); },
    });

    cache.get(); // cold compute
    t = 2000; // past TTL
    cache.get();
    cache.get();
    cache.get();
    expect(scheduled).toHaveLength(1); // only ONE refresh in flight despite 3 stale gets
  });
});
