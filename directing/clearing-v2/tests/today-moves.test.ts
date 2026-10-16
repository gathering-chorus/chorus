// @test-type: unit — Today's "Changed overnight" read from the spine's card moves (Loki), with stubbed doors, no live services.
// @card: #4530
// @domain: messages
//
// The cards door lists no modified time, so moves come from card.item.moved / card.accepted
// in Loki. Replaces a hardcoded list of card numbers. When Loki cannot be read the panel
// says so by name; an empty list never stands in for "nothing changed".
import http from 'http';
import { fetchChangedCards } from '../src/sources/today';

function line(ev: Record<string, unknown>): [string, string] {
  return [String(Date.parse(String(ev.timestamp)) * 1e6), JSON.stringify(ev)];
}

let server: http.Server;
let base = '';
let lokiStatus = 200;

beforeAll(async () => {
  server = http.createServer((req, res) => {
    if ((req.url || '').startsWith('/cards')) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ name: '4552', label: 'no modified time here', status: 'Done' }] }));
      return;
    }
    if ((req.url || '').startsWith('/loki/api/v1/query_range')) {
      res.writeHead(lokiStatus, { 'Content-Type': 'application/json' });
      if (lokiStatus !== 200) { res.end('too many outstanding requests'); return; }
      res.end(JSON.stringify({ data: { result: [{ values: [
        line({ event: 'card.item.moved', card_id: 4530, title: 'Clearing v2', to: 'WIP', role: 'wren', timestamp: '2026-10-15T18:23:37-04:00' }),
        line({ event: 'card.item.moved', card_id: 4558, title: 'verb-smoke', to: 'WIP', role: 'silas', timestamp: '2026-10-15T17:20:07-04:00' }),
        line({ event: 'card.accepted', card_id: 4558, title: 'verb-smoke', role: 'silas', timestamp: '2026-10-15T17:29:00-04:00' }),
      ] }] } }));
      return;
    }
    res.writeHead(404); res.end();
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
});
afterAll(async () => { await new Promise<void>((r) => server.close(() => r())); });

test('moves come from the spine, latest per card, and a land reads landed', async () => {
  lokiStatus = 200;
  const r = await fetchChangedCards(`${base}/cards`, new Date('2026-10-12T05:57:02Z'), base);
  expect(r.unmeasured).toBeUndefined();
  const byId = new Map(r.items.map((c) => [c.id, c]));
  expect(byId.get(4530)?.status).toBe('wip');
  expect(byId.get(4558)?.status).toBe('landed'); // accepted after its WIP move
  expect(r.items.length).toBe(2);
});

test('negative proof: Loki unreadable names it, never an empty "nothing changed"', async () => {
  lokiStatus = 429;
  const r = await fetchChangedCards(`${base}/cards`, new Date('2026-10-12T05:57:02Z'), base);
  expect(r.items).toEqual([]);
  expect(r.unmeasured).toMatch(/card moves unreadable/);
});
