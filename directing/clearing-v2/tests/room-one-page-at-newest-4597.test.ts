// @test-type: unit — builds the room data and page in-process with stand-in relay, router and document; no browser.
// @card: #4597
// @owner: wren
// @domain: messages
/**
 * #4597 — Jeff, 10-18 17:41: "i have to click on right vertical bar 8x to see the
 * latest message", then "i dont want 8 pages worth of chats i want 1 page".
 * The room showed the last 100 messages and opened at the oldest of them.
 */
import { DataService, ROOM_PAGE_MESSAGES } from '../src/data-service';
import http from 'http';
import { renderRoomsPage } from '../src/views';
import { createApp } from '../src/server';
const { scrollRoomToNewest } = require('../public/app.js');

const history = Array.from({ length: 100 }, (_, i) => ({
  id: `m${i}`, from: i % 2 ? 'wren' : 'jeff', text: `message ${i}`, ts: 1760800000000 + i * 1000,
}));
const router = { getRecent: (n: number) => history.slice(-n), getKnownRooms: () => [] as string[] };
const relay = { getTopic: () => 'team', isConnected: () => true, getRelayUrl: () => 'ws://127.0.0.1:1' };
const offline = { rolesUrl: 'http://127.0.0.1:1/roles', pulseUrl: 'http://127.0.0.1:1' };

describe('#4597 the room is one page and opens at the newest message', () => {
  const realDocument = (global as any).document;
  afterEach(() => { (global as any).document = realDocument; });

  test('the room holds one page of the newest messages, not 100', async () => {
    const ds = new DataService({ relay, router, ...offline } as any);
    const data = await ds.getRoomsData('team', offline);
    expect(ROOM_PAGE_MESSAGES).toBeLessThanOrEqual(15);
    expect(data.messages).toHaveLength(ROOM_PAGE_MESSAGES);
    expect(data.messages[data.messages.length - 1].text).toBe('message 99');
    expect(data.messages.map((m) => m.text)).not.toContain('message 0');
  });

  test('the page scrolls its message list to the bottom', async () => {
    const ds = new DataService({ relay, router, ...offline } as any);
    const html = renderRoomsPage('team', await ds.getRoomsData('team', offline), []);
    expect(html).toContain('id="messages-stream"');
    const stream = { scrollTop: 0, scrollHeight: 6800 };
    (global as any).document = { getElementById: (id: string) => (id === 'messages-stream' ? stream : null) };
    scrollRoomToNewest();
    expect(stream.scrollTop).toBe(6800);
  });

  test('the live stream starts with the same one page, not 100', async () => {
    const { app, router, relay: live } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9' });
    for (let i = 0; i < 100; i++) {
      router.ingest({ id: `s${i}`, from: 'jeff', text: `stream ${i}`, ts: new Date(1760800000000 + i * 1000).toISOString(), type: 'jeff-input', visible: true, room: 'team' });
    }
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;
    try {
      const init = await new Promise<any>((resolve, reject) => {
        const req = http.get(`http://127.0.0.1:${port}/api/events`, (res) => {
          let buf = '';
          res.on('data', (c) => {
            buf += String(c);
            const frame = buf.split('\n\n').find((f) => f.startsWith('event: init'));
            if (frame) { req.destroy(); resolve(JSON.parse(frame.split('\n').find((l) => l.startsWith('data: '))!.slice(6))); }
          });
        });
        req.on('error', (e: any) => { if (e.code !== 'ECONNRESET') reject(e); });
      });
      expect(init.messages).toHaveLength(ROOM_PAGE_MESSAGES);
      expect(init.messages[init.messages.length - 1].text).toBe('stream 99');
    } finally {
      live.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });
});
