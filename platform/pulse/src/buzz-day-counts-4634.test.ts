// @test-type: unit — a temp sqlite store, no relay
// @domain: messages
/**
 * #4634 — the messages sensor's Buzz reader. It reads only nudges and Jeff's
 * inputs from pulse's store, and it reports UNMEASURED (exit 3, nothing on
 * stdout) whenever it cannot count, never a day of zeros.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import Database from 'better-sqlite3';
import { WebSocketServer } from 'ws';
import { main, readNotes, readRows } from './buzz-day-counts';

/** A relay that, like the real one, answers only its newest `cap` notes per REQ, honours since/until inclusively, and counts REQs. */
async function cappedRelay(notes: Array<{ id: string; created_at: number }>, cap: number, limitOnReq = 0) {
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  const reqs: unknown[] = [];
  wss.on('connection', (ws) => ws.on('message', (raw) => {
    const f = JSON.parse(String(raw));
    if (f[0] !== 'REQ') return;
    reqs.push(f[2]);
    // like the real relay after a burst: refuse this REQ once, asking for a pause
    if (reqs.length === limitOnReq) { ws.send(JSON.stringify(['CLOSED', f[1], 'rate-limited: quota exceeded; retry in 1s'])); return; }
    const { since = 0, until = Infinity } = f[2];
    const page = notes.filter((n) => n.created_at >= since && n.created_at <= until).sort((a, b) => b.created_at - a.created_at).slice(0, cap);
    for (const n of page) ws.send(JSON.stringify(['EVENT', f[1], { ...n, content: n.id, tags: [['t', 'team']] }]));
    ws.send(JSON.stringify(['EOSE', f[1]]));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, reqs, close: () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); }) };
}

function store(rows: [string, string][]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-days-4634-'));
  const file = path.join(dir, 'messages.db');
  const d = new Database(file);
  d.exec(`CREATE TABLE messages (created_at TEXT, type TEXT, "from" TEXT, "to" TEXT, content TEXT)`);
  const ins = d.prepare(`INSERT INTO messages VALUES (?, ?, 'kade', 'silas', 'hi')`);
  for (const [at, type] of rows) ins.run(at, type);
  d.close();
  return file;
}

describe('#4634 the Buzz reader counts only what can be counted', () => {
  let out: jest.SpyInstance;
  let err: jest.SpyInstance;
  beforeEach(() => {
    out = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    err = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
  });
  afterEach(() => { out.mockRestore(); err.mockRestore(); });

  test('reads nudges and jeff-inputs, and leaves every other row type out', () => {
    const db = store([['2026-10-17 14:00:00', 'nudge'], ['2026-10-17 15:00:00', 'jeff-input'], ['2026-10-17 16:00:00', 'presence']]);
    expect(readRows(db).map((r) => r.type).sort()).toEqual(['jeff-input', 'nudge']);
  });

  test('no relay configured is UNMEASURED: exit 3 and no day printed', async () => {
    const db = store([['2026-10-17 14:00:00', 'nudge']]);
    expect(await main(['node', 'x', '--db', db], {})).toBe(3);
    expect(out).not.toHaveBeenCalled();
    expect(String(err.mock.calls[0][0])).toContain('UNMEASURED');
  });

  test('an empty store is UNMEASURED, not a day of zeros', async () => {
    const db = store([]);
    expect(await main(['node', 'x', '--db', db], { PULSE_BUZZ_RELAY_URL: 'ws://127.0.0.1:1' })).toBe(3);
    expect(out).not.toHaveBeenCalled();
  });

  test('a relay that cannot be reached is UNMEASURED', async () => {
    const db = store([['2026-10-17 14:00:00', 'nudge']]);
    expect(await main(['node', 'x', '--db', db], { PULSE_BUZZ_RELAY_URL: 'ws://127.0.0.1:1' })).toBe(3);
    expect(out).not.toHaveBeenCalled();
  });

  test('reads every note past the relay cap: pages back with until, dedupes the inclusive edge, stops on no new ids', async () => {
    // 10 notes, 3 per page, and three in the same second straddling a page edge
    const notes = [100, 101, 102, 103, 104, 104, 104, 105, 106, 107].map((t, i) => ({ id: `n${i}`, created_at: t }));
    const relay = await cappedRelay(notes, 3);
    try {
      const got = await readNotes(relay.url, undefined, 0, 5000, () => ({ signer: null, reason: 'no key in a test' }) as never);
      expect(got.map((n) => n.content).sort()).toEqual(notes.map((n) => n.id).sort());
      expect(relay.reqs.length).toBeLessThan(10);
    } finally { await relay.close(); }
  });

  test('a rate-limited page is asked for again after the pause, never read as the end', async () => {
    const notes = [100, 101, 102, 103, 104, 105, 106].map((t, i) => ({ id: `n${i}`, created_at: t }));
    const relay = await cappedRelay(notes, 3, 2);
    try {
      const got = await readNotes(relay.url, undefined, 0, 5000, () => ({ signer: null, reason: 'no key in a test' }) as never);
      expect(got).toHaveLength(7);
    } finally { await relay.close(); }
  });

  test('a second holding more notes than one relay page says so on stderr, never drops them quietly', async () => {
    // five notes in second 104, three per page: two cannot be paged by time
    const notes = [100, 101, 104, 104, 104, 104, 104, 106].map((t, i) => ({ id: `n${i}`, created_at: t }));
    const relay = await cappedRelay(notes, 3);
    try {
      const got = await readNotes(relay.url, undefined, 0, 5000, () => ({ signer: null, reason: 'no key in a test' }) as never);
      const said = err.mock.calls.map((c) => String(c[0])).join('');
      expect(said).toContain('stepped past second 104');
      expect(said).toContain('not counted');
      expect(got.length).toBe(notes.length - 2);
    } finally { await relay.close(); }
  });
});
