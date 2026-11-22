// @test-type: unit — a stub relay on a local socket that checks the NIP-42 sign-in; no live relay
// @domain: messages
// @card: #4984
// @owner: silas
/**
 * #4984 — the messages sensor measured nothing every day since 10-26. Since the relay's
 * 10-25 18:01 cutover it accepts a sign-in only when the event's `relay` tag names its
 * public address (wss://…). chorus-crawl ran the reader without BUZZ_AUTH_RELAY_URL, so the
 * reader signed in as ws://<host>, the relay refused, and the reader ignored the refusal
 * and waited out its 60 s. Measured 11-16 04:51 (Loki: "relay sent no EOSE within 60000ms
 * (page 2)", daily) and by probe ("CLOSED … auth-required: not authenticated").
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { readNotes } from './buzz-day-counts';

const HOST = 'buzz.example';
const PUBLIC = `wss://${HOST}`;
const signer = () => ({ signer: { pubkey: 'ab'.repeat(32), sign: () => 'cd'.repeat(64) }, reason: '' }) as never;

/** A relay as Buzz is since 10-25: challenge on connect, REQ refused until a sign-in names PUBLIC. */
async function relay() {
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  const tags: string[] = [];
  wss.on('connection', (ws) => {
    let authed = false;
    ws.send(JSON.stringify(['AUTH', 'challenge-1']));
    ws.on('message', (raw) => {
      const f = JSON.parse((raw as Buffer).toString('utf8'));
      if (f[0] === 'AUTH') {
        const tag = (f[1].tags as string[][]).find((t) => t[0] === 'relay')?.[1] ?? '';
        tags.push(tag);
        authed = tag === PUBLIC;
        ws.send(JSON.stringify(['OK', f[1].id, authed, authed ? '' : 'auth-required: relay tag does not name this relay']));
      } else if (f[0] === 'REQ') {
        ws.send(JSON.stringify(authed ? ['EOSE', f[1]] : ['CLOSED', f[1], 'auth-required: not authenticated']));
      }
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, tags, close: () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); }) };
}

describe('#4984 the messages reader signs in with the relay\'s public address, and a refusal is an error at once', () => {
  const saved = process.env.BUZZ_AUTH_RELAY_URL;
  afterEach(() => { if (saved === undefined) delete process.env.BUZZ_AUTH_RELAY_URL; else process.env.BUZZ_AUTH_RELAY_URL = saved; });

  test('AC1 with BUZZ_AUTH_RELAY_URL the sign-in names the public address and the read completes', async () => {
    process.env.BUZZ_AUTH_RELAY_URL = PUBLIC;
    const r = await relay();
    try {
      await expect(readNotes(r.url, HOST, 0, 60000, signer)).resolves.toEqual([]);
      expect(r.tags).toEqual([PUBLIC]);
    } finally { await r.close(); }
  });

  test('AC2 NEGATIVE PROOF: without it the sign-in names ws://<host>, and the refusal is an error at once, naming it', async () => {
    delete process.env.BUZZ_AUTH_RELAY_URL;
    const r = await relay();
    const t0 = Date.now();
    try {
      await expect(readNotes(r.url, HOST, 0, 60000, signer)).rejects.toThrow(/relay refused (the sign-in|page \d+): auth-required/);
      expect(Date.now() - t0).toBeLessThan(5000);
      expect(r.tags).toEqual([`ws://${HOST}`]);
    } finally { await r.close(); }
  });   // #4915 — no raised timeout (4416): jest's 5 s default fails a slow refusal by name
});
