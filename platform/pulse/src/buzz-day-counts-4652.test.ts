// @test-type: unit — a temp sqlite store and a stub relay on a local socket; no live relay
// @domain: messages
// @card: #4652
// @owner: silas
/**
 * #4652 — writers move to #team channel messages (kind 9, h=<channel id>). The
 * reader counts both kinds so old days keep their counts, and says per day how
 * many kind 1 notes it saw: after the switch, a kind 1 is a writer left behind.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import Database from 'better-sqlite3';
import { WebSocketServer } from 'ws';
import { main, kindOneByDay } from './buzz-day-counts';

const TEAM = '160e1f5d-6816-4c68-94b3-b5f1ade75565';
const AT_1017 = Math.floor(Date.parse('2026-10-17T16:00:00Z') / 1000);

/** A relay that answers each filter by its own kinds and tags, as Buzz does. */
async function relay(notes: Array<{ id: string; kind: number; tags: string[][]; created_at: number; content: string }>) {
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  const filters: Array<Record<string, unknown>> = [];
  wss.on('connection', (ws) => ws.on('message', (raw) => {
    const f = JSON.parse((raw as Buffer).toString('utf8'));
    if (f[0] !== 'REQ') return;
    const flt = f[2] as { kinds: number[]; '#h'?: string[]; '#t'?: string[]; until?: number };
    filters.push(flt);
    const match = (n: (typeof notes)[number]) => flt.kinds.includes(n.kind)
      && (!flt['#h'] || n.tags.some((t) => t[0] === 'h' && flt['#h']!.includes(t[1])))
      && (!flt['#t'] || n.tags.some((t) => t[0] === 't' && flt['#t']!.includes(t[1])))
      && (flt.until === undefined || n.created_at <= flt.until);
    for (const n of notes.filter(match)) ws.send(JSON.stringify(['EVENT', f[1], n]));
    ws.send(JSON.stringify(['EOSE', f[1]]));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, filters, close: () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); }) };
}

function store(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-days-4652-'));
  const file = path.join(dir, 'messages.db');
  const d = new Database(file);
  d.exec('CREATE TABLE messages (created_at TEXT, type TEXT, "from" TEXT, "to" TEXT, content TEXT)');
  d.prepare("INSERT INTO messages VALUES ('2026-10-17 16:00:00', 'nudge', 'kade', 'silas', 'hi')").run();
  d.close();
  return file;
}

describe('#4652 the Buzz reader counts #team channel messages and flags kind 1', () => {
  let out: jest.SpyInstance;
  beforeEach(() => { out = jest.spyOn(process.stdout, 'write').mockImplementation(() => true); jest.spyOn(process.stderr, 'write').mockImplementation(() => true); });
  afterEach(() => { jest.restoreAllMocks(); });

  test('kindOneByDay counts kind 1 notes per Boston day before today, and never a kind 9', () => {
    const n = (kind: number, at: number) => ({ content: 'x', kind, tags: [], created_at: at });
    expect([...kindOneByDay([n(9, AT_1017), n(9, AT_1017 + 60)], '2026-10-18')]).toEqual([]);
    expect([...kindOneByDay([n(1, AT_1017), n(9, AT_1017), n(1, AT_1017 + 60)], '2026-10-18')]).toEqual([['2026-10-17', 2]]);
    expect([...kindOneByDay([n(1, AT_1017)], '2026-10-17')]).toEqual([]);
  });

  test('a kind 9 nudge in #team is kept; the day line says 0 kind 1', async () => {
    const r = await relay([{ id: 'k9', kind: 9, tags: [['h', TEAM], ['t', 'team'], ['author', 'kade'], ['kind', 'nudge']], created_at: AT_1017, content: '[nudge from kade → silas] hi' }]);
    try {
      expect(await main(['node', 'x', '--db', store(), '--today', '2026-10-18'], { PULSE_BUZZ_RELAY_URL: r.url })).toBe(0);
      const line = JSON.parse(String(out.mock.calls[0][0]));
      expect(line).toEqual(expect.objectContaining({ day: '2026-10-17', sent: 1, kept: 1, kind1: 0 }));
      expect(r.filters).toEqual(expect.arrayContaining([expect.objectContaining({ kinds: [9], '#h': [TEAM] })]));
    } finally { await r.close(); }
  });

  test('NEGATIVE PROOF: a kind 1 left by an old writer shows on its day', async () => {
    const r = await relay([{ id: 'old', kind: 1, tags: [['t', 'team'], ['author', 'kade'], ['kind', 'nudge']], created_at: AT_1017, content: '[nudge from kade → silas] hi' }]);
    try {
      expect(await main(['node', 'x', '--db', store(), '--today', '2026-10-18'], { PULSE_BUZZ_RELAY_URL: r.url })).toBe(0);
      expect(JSON.parse(String(out.mock.calls[0][0]))).toEqual(expect.objectContaining({ day: '2026-10-17', kind1: 1 }));
    } finally { await r.close(); }
  });
});
