// @test-type: api — #3442 backfill: starts the real app via startTestApp(); misnamed -unit, actually api.
/**
 * patterns-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/patterns-summary.ts. global fetch is mocked to
 * return canned Loki responses so tests run hermetically (no RUN_INTEGRATION
 * needed). Companion to the existing integration suite in patterns-summary.test.ts.
 */

import { startTestApp, type TestApp } from './lib/test-app';

import { getPatternsSummary, resetPatternsCache } from '../src/patterns-summary';

const realFetch = global.fetch;

function mockFetch(responder: () => Response | Promise<Response>) {
  (global as any).fetch = jest.fn(responder);
}

function makeLokiResponse(streams: Array<Array<[string, string]>>): Response {
  return new Response(JSON.stringify({ data: { result: streams.map((values) => ({ values })) } }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

afterEach(() => {
  (global as any).fetch = realFetch;
  jest.clearAllMocks();
  // #3711 — the TTL cache is module state. Without this, a canned Loki response
  // from one test answers the next test's request and its mock never fires. A
  // test brings its own world; it also takes it away.
  resetPatternsCache();
});

describe('getPatternsSummary', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('empty Loki response → zero counts and empty byDate', async () => {
    mockFetch(() => makeLokiResponse([]));
    const r = await getPatternsSummary(7);
    expect(r.total).toBe(0);
    expect(r.patterns).toEqual({});
    expect(r.byDate).toEqual([]);
    expect(r.days).toBe(7);
  });

  test('single stream with one direction event counts it', async () => {
    const line = JSON.stringify({ pattern: 'direction', timestamp: '2026-04-17T10:00:00Z' });
    mockFetch(() => makeLokiResponse([[['ts1', line]]]));
    const r = await getPatternsSummary(1);
    expect(r.total).toBe(1);
    expect(r.patterns.direction).toBe(1);
    expect(r.byDate).toEqual([{ date: '2026-04-17', total: 1, counts: { direction: 1 } }]);
  });

  test('multiple patterns aggregated across streams', async () => {
    const lines = [
      JSON.stringify({ pattern: 'ideation', timestamp: '2026-04-16T09:00:00Z' }),
      JSON.stringify({ pattern: 'direction', timestamp: '2026-04-17T09:00:00Z' }),
      JSON.stringify({ pattern: 'direction', timestamp: '2026-04-17T10:00:00Z' }),
      JSON.stringify({ pattern: 'demo', timestamp: '2026-04-17T11:00:00Z' }),
    ];
    mockFetch(() => makeLokiResponse([lines.map((l, i) => [`ts${i}`, l] as [string, string])]));
    const r = await getPatternsSummary(30);
    expect(r.total).toBe(4);
    expect(r.patterns).toEqual({ ideation: 1, direction: 2, demo: 1 });
    expect(r.byDate.map((d) => d.date)).toEqual(['2026-04-17', '2026-04-16']);
    const apr17 = r.byDate.find((d) => d.date === '2026-04-17')!;
    expect(apr17.counts).toEqual({ direction: 2, demo: 1 });
  });

  test('missing pattern field falls through to "unknown"', async () => {
    const line = JSON.stringify({ timestamp: '2026-04-17T10:00:00Z' });
    mockFetch(() => makeLokiResponse([[['ts', line]]]));
    const r = await getPatternsSummary(1);
    expect(r.patterns.unknown).toBe(1);
  });

  test('missing timestamp skips byDate entry but still counts total', async () => {
    const line = JSON.stringify({ pattern: 'swat' });
    mockFetch(() => makeLokiResponse([[['ts', line]]]));
    const r = await getPatternsSummary(1);
    expect(r.patterns.swat).toBe(1);
    expect(r.byDate).toEqual([]);
  });

  test('malformed JSON lines are skipped silently', async () => {
    const good = JSON.stringify({ pattern: 'demo', timestamp: '2026-04-17T10:00:00Z' });
    mockFetch(() => makeLokiResponse([[['ts1', 'not json'], ['ts2', good]]]));
    const r = await getPatternsSummary(1);
    expect(r.total).toBe(1);
    expect(r.patterns.demo).toBe(1);
  });

  test('Loki non-OK response returns empty shape', async () => {
    mockFetch(() => new Response('', { status: 500 }));
    const r = await getPatternsSummary(7);
    expect(r.total).toBe(0);
    expect(r.patterns).toEqual({});
  });

  test('Loki network error is caught and returns empty shape', async () => {
    mockFetch(() => { throw new Error('ECONNREFUSED'); });
    const r = await getPatternsSummary(7);
    expect(r.total).toBe(0);
    expect(r.byDate).toEqual([]);
  });

  test('days parameter propagates into response', async () => {
    mockFetch(() => makeLokiResponse([]));
    const r = await getPatternsSummary(30);
    expect(r.days).toBe(30);
  });

  test('fetch URL carries query params for time range and limit', async () => {
    const spy = jest.fn(() => Promise.resolve(makeLokiResponse([])));
    (global as any).fetch = spy;
    await getPatternsSummary(1);
    const url = spy.mock.calls[0][0] as string;
    expect(url).toContain('/loki/api/v1/query_range?');
    expect(url).toContain('limit=500');
    expect(url).toContain('start=');
    expect(url).toContain('end=');
    expect(decodeURIComponent(url)).toContain('interaction.pattern.detected');
  });

  test('streams without a values array are handled gracefully', async () => {
    (global as any).fetch = jest.fn(() =>
      Promise.resolve(new Response(JSON.stringify({ data: { result: [{}] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      })),
    );
    const r = await getPatternsSummary(1);
    expect(r.total).toBe(0);
  });
});

/**
 * #3711 — a dependency failure must not be reported as a fact.
 *
 * The defect: the Loki query genuinely takes ~3.5-3.9s against a 5s
 * AbortSignal.timeout, so it tipped over under any load, and `catch { }`
 * returned {patterns:{}, total:0} — HTTP 200, well-formed, and indistinguishable
 * from "there are genuinely no patterns". Five consecutive live calls on
 * 2026-07-29 went 500/0/0/0/0, each failure landing on exactly 5.00s. The
 * borg-health probe had been correctly reporting this since 2026-07-06 and I
 * had been dismissing it as alarm-battery noise.
 */
describe('#3711 — unavailable is not zero', () => {
  let harness: TestApp;
  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  beforeEach(() => { resetPatternsCache(); });

  test('genuine zero reports source=loki — Loki answered, there is simply nothing', async () => {
    mockFetch(() => makeLokiResponse([]));
    const r = await getPatternsSummary(7);
    expect(r.total).toBe(0);
    expect(r.source).toBe('loki');
  });

  test('a timeout is NOT reported as zero patterns', async () => {
    mockFetch(() => Promise.reject(new DOMException('The operation was aborted.', 'TimeoutError')));
    const r = await getPatternsSummary(7);
    expect(r.source).toBe('unavailable');
    expect(r.reason).toBeTruthy();
    // and it is distinguishable from the genuine-zero case above
    expect(r.source).not.toBe('loki');
  });

  test('a non-ok Loki response is unavailable, not zero', async () => {
    mockFetch(() => new Response('boom', { status: 503 }));
    const r = await getPatternsSummary(7);
    expect(r.source).toBe('unavailable');
    expect(r.reason).toContain('503');
  });

  test('the slow query is paid once, not per request (TTL cache)', async () => {
    const line = JSON.stringify({ pattern: 'demo', timestamp: '2026-07-29T10:00:00Z' });
    const spy = jest.fn(() => Promise.resolve(makeLokiResponse([[['ts', line]]])));
    (global as any).fetch = spy;
    const a = await getPatternsSummary(30);
    const b = await getPatternsSummary(30);
    const c = await getPatternsSummary(30);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(a.source).toBe('loki');
    expect(b.source).toBe('cache');
    expect(c.total).toBe(a.total);
  });

  test('a refresh failure serves the last good value marked stale, never a false zero', async () => {
    const line = JSON.stringify({ pattern: 'reflection', timestamp: '2026-07-29T10:00:00Z' });
    let fail = false;
    (global as any).fetch = jest.fn(() =>
      fail ? Promise.reject(new Error('loki down'))
           : Promise.resolve(makeLokiResponse([[['ts', line]]])));
    const good = await getPatternsSummary(30);
    expect(good.total).toBe(1);
    resetPatternsCache({ keepValue: true });  // cache present but stale
    fail = true;
    const after = await getPatternsSummary(30);
    expect(after.total).toBe(1);          // the last good value, not 0
    expect(after.source).toBe('cache');
    expect(after.stale).toBe(true);
  });

  test('different day-windows are cached separately', async () => {
    const line = JSON.stringify({ pattern: 'demo', timestamp: '2026-07-29T10:00:00Z' });
    const spy = jest.fn(() => Promise.resolve(makeLokiResponse([[['ts', line]]])));
    (global as any).fetch = spy;
    await getPatternsSummary(7);
    await getPatternsSummary(30);
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
