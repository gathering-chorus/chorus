// @test-type: unit — router, routes and pages in-process with a local HTTP stub; no browser.
// @card: #4597
// @owner: wren
// @domain: messages
// #4766 (AC3): the Streams tab renders with the old Clearing stopped
// #4986 (AC1): one stream, every role interleaved, oldest at top, newest at the bottom
/**
 * #4597 — Jeff 19:11: "i cant scroll up in message history and really need streams tab".
 * Scrolling up loads the page just older than the oldest shown; the Streams tab shows
 * every role's recent commands as one stream, newest at the bottom (#4986), and names an
 * unreadable stream.
 */
import http from 'http';
import { createApp } from '../src/server';
import { ROOM_PAGE_MESSAGES } from '../src/data-service';
import { fetchStreams } from '../src/sources/streams';
import { renderStreamsPage, STREAM_TAIL_SCRIPT } from '../src/views';
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
  // #4986 (AC1) — Jeff 11-17: "interleaved across all of u most recent at bottom", like tail -f
  test('every role in one stream, oldest at top, newest at the bottom, each line naming its role', () => {
    const lines = [
      { ts: '7:10 PM', role: 'kade', text: 'bash: one' },
      { ts: '7:11 PM', role: 'wren', text: 'bash: two' },
      { ts: '7:12 PM', role: 'kade', text: 'bash: three' },
    ];
    const html = renderStreamsPage({ ok: true, lines }, []);
    expect(html).toContain('href="/streams" class="on"');
    const at = (t: string) => html.indexOf(t);
    expect(at('bash: one')).toBeLessThan(at('bash: two'));
    expect(at('bash: two')).toBeLessThan(at('bash: three'));
    expect(html).toMatch(/<b class="who-wren">wren<\/b> bash: two/);
    // NEGATIVE PROOF: the lines come out exactly in time order across roles. A per-role grouped
    // render gives kade one, kade three, wren two; newest-first gives three, two, one. Both fail
    // this sequence (both mutations were run red).
    const order = Array.from(html.matchAll(/bash: (one|two|three)/g)).map((m) => m[1]);
    expect(order).toEqual(['one', 'two', 'three']);
  });

  // #4986 (AC1) — tail -f on the phone: the stream refreshes in place and follows the bottom only
  // while the reader is there; scrolled up to read back, the reader keeps their place.
  type Resp = { ok: boolean; status?: number; body: string } | Error | 'hang';
  function runTail(scrollY: number, resp: Resp = { ok: true, body: 'new' }, hasTail = true) {
    const scrolls: number[] = [];
    let reloads = 0;
    let fetches = 0;
    let tick: () => void = () => undefined;
    const timers: Array<() => void> = [];
    const stale: Array<{ textContent: string }> = [];
    const cur = { innerHTML: 'old', after: (n: { textContent: string }) => { stale.push(n); } };
    const win = { innerHeight: 800, scrollY, location: { href: 'http://x/streams', reload: () => { reloads++; } }, scrollTo: (_x: number, y: number) => { scrolls.push(y); } };
    const doc = {
      body: { scrollHeight: 2000 },
      getElementById: (id: string) => (id === 'stream-tail' && hasTail ? cur : id === 'stream-stale' ? stale[0] ?? null : null),
      createElement: () => ({ id: '', className: '', textContent: '' }),
    };
    const parser = function DOMParser(this: unknown) {
      return { parseFromString: (h: string) => ({ getElementById: () => (h === 'no-stream' ? null : { innerHTML: h }) }) };
    };
    class Ctl { signal = { aborted: false, onabort: null as null | (() => void) }; abort() { this.signal.aborted = true; this.signal.onabort?.(); } }
    const fetchStub = (_u: string, o: { signal: Ctl['signal'] }) => {
      fetches++;
      if (resp === 'hang') return new Promise((_r, rej) => { o.signal.onabort = () => { const e = new Error('aborted'); e.name = 'AbortError'; rej(e); }; });
      if (resp instanceof Error) return Promise.reject(resp);
      return Promise.resolve({ ok: resp.ok, status: resp.status ?? (resp.ok ? 200 : 502), text: () => Promise.resolve(resp.body) });
    };
    new Function('window', 'document', 'fetch', 'DOMParser', 'setInterval', 'setTimeout', 'clearTimeout', 'AbortController', STREAM_TAIL_SCRIPT)(
      win, doc, fetchStub, parser, (fn: () => void) => { tick = fn; }, (fn: () => void) => { timers.push(fn); return timers.length; }, () => undefined, Ctl);
    scrolls.length = 0; // the first load always opens at the bottom
    tick();
    return { scrolls, tick: () => tick(), fireTimers: () => timers.splice(0).forEach((f) => f()), swapped: () => cur.innerHTML, reloads: () => reloads, fetches: () => fetches, stale: () => stale.map((x) => x.textContent) };
  }
  const settle = () => new Promise((r) => setImmediate(r));

  test('at the bottom, a refresh swaps the stream in place and follows the newest line', async () => {
    const t = runTail(1200); // 800 + 1200 = bottom of 2000
    await settle();
    expect(t.swapped()).toBe('new');
    expect(t.scrolls).toEqual([2000]);
  });

  test('NEGATIVE PROOF: scrolled up, a refresh swaps the stream but never moves the reader', async () => {
    const t = runTail(100);
    await settle();
    expect(t.swapped()).toBe('new');
    expect(t.scrolls).toEqual([]);
  });

  test('NEGATIVE PROOF: a 200 page with no stream reloads so the reason is named, never stale lines', async () => {
    const gone = runTail(1200, { ok: true, body: 'no-stream' });
    await settle();
    expect(gone.reloads()).toBe(1);
    expect(gone.swapped()).toBe('old');
    const none = runTail(1200, { ok: true, body: 'new' }, false);
    expect(none.reloads()).toBe(1);
  });

  test('NEGATIVE PROOF: an error status (a 502 during a restart) marks the stream stale and never reloads', async () => {
    const t = runTail(1200, { ok: false, status: 502, body: 'bad gateway' });
    await settle();
    expect(t.reloads()).toBe(0);
    expect(t.stale()).toEqual(['Not refreshed: answered 502']);
    expect(t.swapped()).toBe('old');
  });

  test('NEGATIVE PROOF: a failed re-read marks the stream not refreshed, with the reason', async () => {
    const t = runTail(1200, new Error('network down'));
    await settle();
    expect(t.stale()).toEqual(['Not refreshed: network down']);
    expect(t.swapped()).toBe('old');
  });

  test('NEGATIVE PROOF: one read at a time, and a hung read is abandoned so the tail keeps going', async () => {
    const t = runTail(1200, 'hang');
    t.tick(); // a second tick while the first read hangs sends nothing
    expect(t.fetches()).toBe(1);
    t.fireTimers(); // 8 s pass: the hung read is abandoned
    await settle();
    expect(t.stale()).toEqual(['Not refreshed: no answer in 8 s']);
    t.tick(); // the tail is free again
    expect(t.fetches()).toBe(2);
  });

  test('NEGATIVE PROOF: an unreachable stream is named, never an empty page', async () => {
    const streams = await fetchStreams('http://127.0.0.1:1/api/stream');
    expect(streams.ok).toBe(false);
    const html = renderStreamsPage(streams, []);
    expect(html).toContain('Streams could not be read: stream unreachable');
  });
});

// #4766 AC3 — the Streams tab renders each role's calls with the old Clearing stopped: the
// only upstream is a stub of the generated route; nothing listens on :3470.
describe('#4766 Streams without the old Clearing', () => {
  async function stubRoute(body: unknown, code = 200): Promise<{ url: string; seen: string[]; close: () => Promise<void> }> {
    const seen: string[] = [];
    const s = http.createServer((req, res) => { seen.push(req.url || ''); res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(body)); });
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    return { url: `http://127.0.0.1:${(s.address() as { port: number }).port}/pulse/streams/events`, seen, close: () => new Promise((r) => s.close(() => r())) };
  }
  const rows = {
    data: [
      { name: 'c3', label: 'cargo test', streamRole: 'wren', streamTool: 'Bash', streamPhase: 'running', streamTs: '2026-11-17T01:00:03Z' },
      { name: 'c2', label: 'git status', streamRole: 'kade', streamTool: 'Bash', streamPhase: 'failed', streamTs: '2026-11-17T01:00:02Z' },
      { name: 'c1', label: 'read card', streamRole: 'wren', streamTool: 'Read', streamPhase: 'done', streamTs: '2026-11-17T01:00:01Z' },
    ],
  };

  test('calls come back as one stream, newest last, read with ?limit=200', async () => {
    const r = await stubRoute(rows);
    try {
      const got = await fetchStreams(r.url);
      expect(r.seen).toEqual(['/pulse/streams/events?limit=200']);
      expect(got.ok).toBe(true);
      if (!got.ok) return;
      expect(got.lines.map((l) => `${l.role} ${l.text}`)).toEqual(['wren read card', 'kade git status \u2717', 'wren \u23f3 cargo test']);
      expect(got.lines[2].ts).toBe('Nov 16 20:00'); // 11-17 01:00 UTC is 11-16 20:00 Boston, so the date shows (#4991)
    } finally { await r.close(); }
  });

  test('the /streams page renders from the generated route with nothing on :3470', async () => {
    const r = await stubRoute(rows);
    const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null, streamUrl: r.url });
    const server = http.createServer(app);
    await new Promise<void>((ok) => server.listen(0, '127.0.0.1', () => ok()));
    try {
      const page = await (await fetch(`http://127.0.0.1:${(server.address() as { port: number }).port}/clearing-v2/streams`)).text();
      expect(page).toContain('cargo test');
      expect(page).toContain('git status');
      expect(page).not.toContain('Streams could not be read');
    } finally {
      await new Promise<void>((ok) => server.close(() => ok()));
      await r.close();
    }
  });

  test('NEGATIVE PROOF: a route that answers without a data list is named, never an empty tab', async () => {
    const r = await stubRoute([{ ts: '7:10 PM', role: 'kade', text: 'old shape' }]);
    try {
      const got = await fetchStreams(r.url);
      expect(got).toEqual({ ok: false, reason: 'stream returned no list' });
    } finally { await r.close(); }
  });
});
