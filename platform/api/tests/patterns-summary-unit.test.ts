/**
 * patterns-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/patterns-summary.ts. global fetch is mocked to
 * return canned Loki responses so tests run hermetically (no RUN_INTEGRATION
 * needed). Companion to the existing integration suite in patterns-summary.test.ts.
 */

import { startTestApp, type TestApp } from './lib/test-app';

import { getPatternsSummary } from '../src/patterns-summary';

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
