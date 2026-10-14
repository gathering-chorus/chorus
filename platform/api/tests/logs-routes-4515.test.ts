// @test-type: unit — drives chorus-api's logs routes in-process against a fake Loki on loopback
// @domain: logs
// @card: #4515
// @owner: wren
/**
 * #4515 — the old MCP server built the logs tools' LogQL itself; with that
 * server gone, the runner's logs tools call these routes. What an agent sees:
 * the window it asks for is the window Loki is asked for, recent errors can
 * be narrowed to a role, and a window the route does not know is refused.
 */
import * as fs from 'fs';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-4515-'));
const LOKI_PORT = 40000 + (process.pid % 20000);
process.env.CHORUS_ROOT = TMP;
process.env.DB_PATH = path.join(TMP, 'test.db');
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.LOKI_URL = `http://127.0.0.1:${LOKI_PORT}`;

import app from '../src/server';
import type { AddressInfo } from 'net';
import type { Server } from 'http';

const asked: URL[] = [];
const loki = http.createServer((req, res) => {
  asked.push(new URL(req.url ?? '/', 'http://loki'));
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ data: { result: [] } }));
});
beforeAll(async () => {
  await new Promise<void>((r) => loki.listen(LOKI_PORT, '127.0.0.1', () => r()));
});
afterAll(async () => {
  await new Promise<void>((r) => loki.close(() => r()));
});

/** The window Loki was asked for, in seconds. */
function span(u: URL): number {
  return Math.round((Number(u.searchParams.get('end')) - Number(u.searchParams.get('start'))) / 1e9);
}

/** One request to the running app; returns its status and what it asked Loki. */
async function ask(srv: Server, p: string): Promise<{ status: number; loki: URL | undefined }> {
  asked.length = 0;
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  try {
    const res = await fetch(`http://127.0.0.1:${(srv.address() as AddressInfo).port}${p}`);
    await res.json();
    return { status: res.status, loki: asked[0] };
  } finally {
    await new Promise<void>((r) => srv.close(() => r()));
  }
}

describe('#4515 the logs routes the runner calls', () => {
  test('a card log read covers the window asked for', async () => {
    const r = await ask(app.listen(0), '/api/chorus/logs/card/4515?window=1h');
    expect(r.status).toBe(200);
    expect(r.loki!.searchParams.get('query')).toContain('card_id');
    expect(r.loki!.searchParams.get('query')).toContain('4515');
    expect(span(r.loki!)).toBe(3600);
  });

  test('no window keeps the old default of a day', async () => {
    expect(span((await ask(app.listen(0), '/api/chorus/logs/trace/abc123')).loki!)).toBe(86400);
  });

  test('recent errors, narrowed to one role', async () => {
    const r = await ask(app.listen(0), '/api/chorus/logs/errors?role=kade&window=6h');
    expect(r.status).toBe(200);
    const q = r.loki!.searchParams.get('query')!;
    expect(q).toContain('error');
    expect(q).toContain('kade');
    expect(span(r.loki!)).toBe(21600);
  });

  test('recent errors default to the last hour, every role', async () => {
    const r = await ask(app.listen(0), '/api/chorus/logs/errors');
    expect(span(r.loki!)).toBe(3600);
    expect(r.loki!.searchParams.get('query')).not.toContain('"role');
  });

  test.each(['/api/chorus/logs/card/4515?window=1y', '/api/chorus/logs/trace/abc?window=forever', '/api/chorus/logs/errors?window=2h'])(
    'NEGATIVE PROOF: %s is refused by name and Loki is never asked',
    async (p) => {
      const r = await ask(app.listen(0), p);
      expect(r.status).toBe(400);
      expect(r.loki).toBeUndefined();
    },
  );
});
