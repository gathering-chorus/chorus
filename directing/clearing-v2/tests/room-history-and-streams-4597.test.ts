// @test-type: unit — router, routes and pages in-process with a local HTTP stub; no browser.
// @card: #4597
// @owner: wren
// @domain: messages
/**
 * #4597 — Jeff 19:11: "i cant scroll up in message history and really need streams tab".
 * Scrolling up loads the page just older than the oldest shown; the Streams tab shows
 * each role's recent commands, newest first, and names an unreadable stream.
 */
import http from 'http';
import { createApp } from '../src/server';
import { ROOM_PAGE_MESSAGES } from '../src/data-service';
import { groupByRole, fetchStreams } from '../src/sources/streams';
import { renderStreamsPage } from '../src/views';
const { loadOlderMessages } = require('../public/app.js');

const msg = (i: number) => ({ id: `h${i}`, from: i % 2 ? 'wren' : 'jeff', text: `history ${i}`, ts: new Date(1760800000000 + i * 1000).toISOString(), type: 'jeff-input' as const, visible: true, room: 'team' });

async function serve(app: http.RequestListener) {
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { server, base: `http://127.0.0.1:${(server.address() as any).port}` };
}

describe('#4597 scrolling up loads older messages', () => {
  test('history before the oldest shown message is the page just older, oldest first', async () => {
    const { app, router, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: 'http://127.0.0.1:9' });
    for (let i = 0; i < 60; i++) router.ingest(msg(i));
    const { server, base } = await serve(app);
    try {
      const oldestShown = `h${60 - ROOM_PAGE_MESSAGES}`;
      const body = await (await fetch(`${base}/api/rooms/team/history?before=${oldestShown}`)).json() as { count: number; html: string };
      expect(body.count).toBe(ROOM_PAGE_MESSAGES);
      expect(body.html).toContain('id="msg-h30"');
      expect(body.html).toContain(`id="msg-h${60 - ROOM_PAGE_MESSAGES - 1}"`);
      expect(body.html).not.toContain(`id="msg-${oldestShown}"`);
      const first = await (await fetch(`${base}/api/rooms/team/history?before=h0`)).json() as { count: number };
      expect(first.count).toBe(0);
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('the room puts the older page above the oldest message and keeps the reader in place', async () => {
    const inserted: string[] = [];
    const stream = { scrollTop: 0, scrollHeight: 1000, querySelector: () => first };
    // the older page makes the list 600px taller
    const first = { id: 'msg-h45', insertAdjacentHTML: (_where: string, html: string) => { inserted.push(html); stream.scrollHeight = 1600; } };
    (global as any).document = { getElementById: (id: string) => (id === 'messages-stream' ? stream : id === 'room-picker' ? { value: 'team' } : null) };
    let asked = '';
    const added = await loadOlderMessages(async (url: string) => {
      asked = url;
      return { ok: true, json: async () => ({ count: 15, html: '<div id="msg-h30"></div>' }) };
    });
    expect(asked).toBe('/api/rooms/team/history?before=h45');
    expect(added).toBe(15);
    expect(inserted).toEqual(['<div id="msg-h30"></div>']);
    expect(stream.scrollTop).toBe(600);
    delete (global as any).document;
  });
});

describe('#4597 the Streams tab', () => {
  test('commands are grouped one column per role, newest first', () => {
    const roles = groupByRole([
      { ts: '7:10 PM', role: 'kade', text: 'bash: one' },
      { ts: '7:11 PM', role: 'wren', text: 'bash: two' },
      { ts: '7:12 PM', role: 'kade', text: 'bash: three' },
    ]);
    expect(roles.map((r) => r.role)).toEqual(['kade', 'wren']);
    expect(roles[0].lines.map((l) => l.text)).toEqual(['bash: three', 'bash: one']);
    const html = renderStreamsPage({ ok: true, roles }, []);
    expect(html).toContain('href="/streams" class="on"');
    expect(html.indexOf('bash: three')).toBeLessThan(html.indexOf('bash: one'));
  });

  test('NEGATIVE PROOF: an unreachable stream is named, never an empty page', async () => {
    const streams = await fetchStreams('http://127.0.0.1:1/api/stream');
    expect(streams.ok).toBe(false);
    const html = renderStreamsPage(streams, []);
    expect(html).toContain('Streams could not be read: stream unreachable');
  });
});
