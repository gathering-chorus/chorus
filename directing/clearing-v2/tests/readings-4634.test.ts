// @test-type: unit — fixture readings and a local HTTP stub for the door; no live store.
// @card: #4634
// @owner: silas
// @domain: messages
/**
 * #4634 — /counts shows messages and events by day from the herald sensors'
 * readings, and derives nothing. With no reading it says unmeasured and why,
 * never 0 (Wren, 2026-10-18).
 */
import http from 'http';
import { byDay, DAY_COLUMNS } from '../src/sources/readings';
import { renderCountsPage } from '../src/views';
import { createApp } from '../src/server';

const r = (sensor: string, metric: string, day: string, value: number, role?: string) => ({ readingOfSensor: sensor, readingMetric: metric, readingDay: day, readingValue: String(value), readingRole: role });

describe('#4634 /counts presents the readings by day', () => {
  test('one row per day, newest first, team totals only', () => {
    const days = byDay([
      r('messages', 'messages-sent', '2026-10-16', 789), r('messages', 'messages-kept', '2026-10-16', 0),
      r('events', 'events', '2026-10-17', 156021), r('messages', 'nudges', '2026-10-17', 300, 'kade'),
    ]);
    expect(days.map((d) => d.day)).toEqual(['2026-10-17', '2026-10-16']);
    const col = (m: string) => DAY_COLUMNS.findIndex(([, metric]) => metric === m);
    expect(days[1].values[col('messages-sent')]).toBe(789);
    expect(days[0].values[col('events')]).toBe(156021);
    expect(days[0].values[col('nudges')]).toBeNull();
  });

  test('a day with none of the table\'s readings (today, backups only) is not shown as a row of unmeasured', () => {
    expect(byDay([r('messages', 'backup-age', '2026-10-18', 2), r('events', 'events', '2026-10-17', 5)]).map((d) => d.day)).toEqual(['2026-10-17']);
  });

  test('a column with no reading shows unmeasured, never 0; a measured 0 shows 0', () => {
    const html = renderCountsPage(byDay([r('messages', 'messages-kept', '2026-10-16', 0)]), {}, []);
    expect(html).toContain('unmeasured');
    expect(html).toMatch(/<td class="num[^"]*">0<\/td>/);
  });

  test('NEGATIVE PROOF: no readings at all is "Unmeasured" with the reason, and no zeros', async () => {
    const door = http.createServer((_q, s) => { s.writeHead(200, { 'Content-Type': 'application/json' }); s.end('{"data":[]}'); });
    await new Promise<void>((ok) => door.listen(0, '127.0.0.1', () => ok()));
    const dport = (door.address() as { port: number }).port;
    const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', knownKeys: new Map(), readingsUrl: `http://127.0.0.1:${dport}/r` });
    const server = http.createServer(app);
    await new Promise<void>((ok) => server.listen(0, '127.0.0.1', () => ok()));
    const port = (server.address() as { port: number }).port;
    try {
      const html = await (await fetch(`http://127.0.0.1:${port}/counts`)).text();
      expect(html).toContain('Unmeasured: the sensors have written no readings yet');
      expect(html).not.toMatch(/<td[^>]*>0<\/td>/);
    } finally {
      await new Promise<void>((ok) => server.close(() => ok()));
      await new Promise<void>((ok) => door.close(() => ok()));
    }
  });

  test('an unreadable door is named on the page', () => {
    const html = renderCountsPage([], { error: 'the readings door answered 404' }, []);
    expect(html).toContain('Unmeasured: the readings door answered 404');
  });
});
