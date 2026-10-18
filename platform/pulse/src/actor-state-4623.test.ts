// @test-type: integration — a local HTTP stub stands in for Loki and the door
/**
 * #4623 — actor state's 30-minute Loki count is read at most once per
 * ACTION_30M_REFRESH_MS, not on every 15s tick. The 5-minute count stays live.
 * These tests count the windows each tick asks Loki for.
 */
import http from 'http';
import { buildLiveActorFactFetcher, ACTION_30M_REFRESH_MS } from './actor-state';

const DEAD = 'http://127.0.0.1:1';
const store = { getOwedNudges: () => [], queryMessages: () => [] };
const deadSources = { store, readTail: () => Promise.resolve(''), athenaBaseUrl: DEAD, chorusApiBaseUrl: DEAD, daguBaseUrl: DEAD };

async function fakeLoki(): Promise<{ base: string; windows: string[]; close: () => Promise<void> }> {
  const windows: string[] = [];
  const srv = http.createServer((req, res) => {
    const q = new URL(req.url || '/', 'http://x').searchParams.get('query') || '';
    const m = q.match(/\[(\d+m)\]/);
    if (m) windows.push(m[1]);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ data: { result: [{ metric: { role: 'kade' }, value: [0, m?.[1] === '30m' ? '9' : '2'] }] } }));
  });
  await new Promise<void>((r) => srv.listen(0, '127.0.0.1', () => r()));
  const base = `http://127.0.0.1:${(srv.address() as { port: number }).port}`;
  return { base, windows, close: () => new Promise<void>((r) => srv.close(() => r())) };
}

describe('#4623 actor state reads the 30-minute count at most once per refresh', () => {
  test('three quick ticks ask for 5m three times and 30m once, and keep the 30m count', async () => {
    const loki = await fakeLoki();
    try {
      const fetcher = buildLiveActorFactFetcher({ ...deadSources, lokiBaseUrl: loki.base });
      await fetcher();
      await fetcher();
      const third = await fetcher();
      expect(loki.windows.filter((w) => w === '5m').length).toBe(3);
      expect(loki.windows.filter((w) => w === '30m').length).toBe(1);
      expect(third.actions?.kade?.recent30mCount).toBe(9);
      expect(ACTION_30M_REFRESH_MS).toBeLessThanOrEqual(2 * 60_000);
    } finally {
      await loki.close();
    }
  });

  test('LOKI_URL=off asks Loki nothing and reports activity unmeasured', async () => {
    const input = await buildLiveActorFactFetcher({ ...deadSources, lokiBaseUrl: 'off' })();
    expect(input.unmeasured?.find((u) => u.source === 'loki:activity')?.reason).toBe('loki off in this environment');
  });
});
