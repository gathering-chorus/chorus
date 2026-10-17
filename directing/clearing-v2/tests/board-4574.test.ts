// @test-type: unit — a local stub server stands in for the board API; no live services
// @domain: cards
// @card: #4574
// @owner: wren
// #4574 — the Board tab shows the board API's columns; #N links open the card; an
// unreadable column says unmeasured with the reason, never "no cards".
import http from 'http';
import { createApp } from '../src/server';
import { fetchBoard } from '../src/sources/board';

async function stubBoard(handler: (url: string) => [number, unknown]): Promise<{ url: string; close: () => Promise<void> }> {
  const s = http.createServer((req, res) => { const [code, body] = handler(req.url || ''); res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(body)); });
  await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
  return { url: `http://127.0.0.1:${(s.address() as { port: number }).port}/board`, close: () => new Promise((r) => s.close(() => r())) };
}

const cards = (ids: number[]) => ({ data: { total: ids.length, cards: ids.map((id) => ({ id, title: `card ${id}`, owner: 'Wren' })) } });

test('columns come from the API as served; Done names its missing source', async () => {
  const b = await stubBoard((u) => (u.endsWith('/wip') ? [200, cards([4574])] : [200, cards([3198, 4501])]));
  try {
    const cols = await fetchBoard(b.url);
    expect(cols.map((c) => c.name)).toEqual(['WIP', 'Next', 'Done']);
    expect(cols[0]).toMatchObject({ ok: true, cards: [{ id: 4574, title: 'card 4574' }] });
    expect(cols[1].ok && cols[1].cards.map((c) => c.id)).toEqual([3198, 4501]);
    expect(cols[2]).toMatchObject({ ok: false, reason: expect.stringContaining('no board API') });
  } finally { await b.close(); }
});

test('the /board page lists cards with links that open them, under the base path too', async () => {
  const b = await stubBoard((u) => (u.endsWith('/wip') ? [200, cards([4574])] : [200, cards([3198])]));
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null, boardUrl: b.url });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  try {
    const page = await (await fetch(`http://127.0.0.1:${port}/clearing-v2/board?card=3198`)).text();
    expect(page).toContain('#4574');
    expect(page).toContain('card 3198');
    expect(page).toContain("openCardDrawer('3198')");
    expect(page).toContain('data-open-card="3198"');
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
    await b.close();
  }
});

test('negative proof: with the board API down each column reads unmeasured, never no cards', async () => {
  const cols = await fetchBoard('http://127.0.0.1:9/board');
  for (const c of cols) {
    expect(c.ok).toBe(false);
  }
  const b = await stubBoard(() => [503, { error: 'down' }]);
  try {
    const down = await fetchBoard(b.url);
    expect(down[0]).toMatchObject({ ok: false, reason: 'board API answered 503' });
  } finally { await b.close(); }
});
