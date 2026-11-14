// @domain: awareness
// @test-type: unit — readNotes in buzz-day-counts.ts against a stub relay on 127.0.0.1; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the day-count reader against a relay that asks for AUTH: with no
 * bridge key it stops and names why (unmeasured, never an empty count); with a
 * key it answers the challenge as the bridge and then reads the room; a relay
 * that never ends a page is a named timeout.
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { readNotes, KIND_ONE_FILTER } from './buzz-day-counts';
import type { BuzzNote } from './buzz-writer';

const sk = new Uint8Array(32).fill(11);
const signer = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

let savedAuthUrl: string | undefined;
beforeAll(() => { savedAuthUrl = process.env.BUZZ_AUTH_RELAY_URL; delete process.env.BUZZ_AUTH_RELAY_URL; });
afterAll(() => { if (savedAuthUrl !== undefined) process.env.BUZZ_AUTH_RELAY_URL = savedAuthUrl; });

/** A relay that challenges first and, when `answer`, answers each REQ with `notes` then EOSE. */
async function relay(opts: { challenge: boolean; answer: boolean; notes?: Array<{ id: string; created_at: number; content: string; kind: number; tags: string[][] }> }) {
  const frames: unknown[][] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => {
    ws.send('{not json');
    if (opts.challenge) ws.send(JSON.stringify(['AUTH', 'day-challenge']));
    ws.on('message', (raw) => {
      const f = JSON.parse((raw as Buffer).toString('utf8')) as unknown[];
      frames.push(f);
      if (f[0] !== 'REQ' || !opts.answer) return;
      const until = (f[2] as { until?: number }).until;
      for (const n of (opts.notes ?? []).filter((x) => until === undefined || x.created_at <= until)) ws.send(JSON.stringify(['EVENT', f[1], n]));
      ws.send(JSON.stringify(['EOSE', f[1]]));
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  const close = () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); });
  return { url, frames, close };
}

describe('#4965 readNotes against a relay that asks for AUTH', () => {
  test('negative: with no bridge key the read rejects with the signer\'s reason', async () => {
    const r = await relay({ challenge: true, answer: true });
    try {
      await expect(readNotes(r.url, undefined, 0, 400, () => ({ reason: 'no bridge key at /nowhere' }))).rejects.toThrow('no bridge key at /nowhere');
      expect(r.frames.some((f) => f[0] === 'AUTH')).toBe(false);
    } finally { await r.close(); }
  });

  test('negative: a signer answer with neither key nor reason still rejects as "no bridge key"', async () => {
    const r = await relay({ challenge: true, answer: true });
    try {
      await expect(readNotes(r.url, undefined, 0, 400, () => ({}))).rejects.toThrow('no bridge key');
    } finally { await r.close(); }
  });

  test('with a bridge key it answers the challenge, then reads every note once', async () => {
    const notes = [
      { id: 'a'.repeat(64), created_at: 1_760_000_100, content: 'one', kind: 1, tags: [['t', 'team']] },
      { id: 'b'.repeat(64), created_at: 1_760_000_200, content: 'two', kind: 1, tags: [['t', 'team']] },
    ];
    const r = await relay({ challenge: true, answer: true, notes });
    try {
      const got = await readNotes(r.url, '127.0.0.1:9999', 1_760_000_000, 2000, () => ({ signer }), KIND_ONE_FILTER);
      expect(got.map((n) => n.content).sort()).toEqual(['one', 'two']);
      const auth = r.frames.find((f) => f[0] === 'AUTH')![1] as BuzzNote;
      expect(auth.kind).toBe(22242);
      expect(auth.tags).toEqual([['relay', 'ws://127.0.0.1:9999'], ['challenge', 'day-challenge']]);
      expect(schnorr.verify(auth.sig, auth.id, signer.pubkey)).toBe(true);
      expect(r.frames.filter((f) => f[0] === 'AUTH')).toHaveLength(1);
    } finally { await r.close(); }
  });

  test('negative: a relay that never ends the page is a named timeout', async () => {
    const r = await relay({ challenge: false, answer: false });
    try {
      await expect(readNotes(r.url, undefined, 0, 120, () => ({ signer }))).rejects.toThrow('relay sent no EOSE within 120ms (page 0)');
    } finally { await r.close(); }
  });
});
