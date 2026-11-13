// @domain: awareness
// @test-type: unit — BuzzWriter, roomText, frameText and bridgeSignerFromHome in buzz-writer.ts against a stub relay on 127.0.0.1 and temp key files; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the Buzz writer answers the relay's AUTH challenge as the bridge,
 * ignores frames that are not its OK, reports a note the relay never answers
 * as a miss naming the timeout, and reports a dropped connection on the next
 * post. The bridge key is read from the home's key file, and a file that holds
 * no usable key is named, never a signer.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import WebSocket, { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { BuzzWriter, bridgeSignerFromHome, frameText, roomText, signNote, type BuzzNote } from './buzz-writer';

const sk = new Uint8Array(32).fill(7);
const signer = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

let savedAuthUrl: string | undefined;
beforeAll(() => { savedAuthUrl = process.env.BUZZ_AUTH_RELAY_URL; delete process.env.BUZZ_AUTH_RELAY_URL; });
afterAll(() => { if (savedAuthUrl !== undefined) process.env.BUZZ_AUTH_RELAY_URL = savedAuthUrl; });

/** A relay that greets with junk and an AUTH challenge, and lets the test answer each EVENT. */
async function relay(onEvent: (ws: WebSocket, note: BuzzNote) => void) {
  const frames: unknown[][] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => {
    ws.send('not json at all');
    ws.send(JSON.stringify(['AUTH', 'challenge-4965']));
    ws.on('message', (data) => {
      const f = JSON.parse((data as Buffer).toString('utf8')) as unknown[];
      frames.push(f);
      if (f[0] === 'EVENT') onEvent(ws, f[1] as BuzzNote);
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  const close = () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); });
  return { url, frames, wss, close };
}

const until = async (cond: () => boolean, ms = 3000) => {
  for (let t = 0; t < ms && !cond(); t += 20) await new Promise((r) => setTimeout(r, 20));
};

describe('#4965 the writer talks to the relay as the bridge', () => {
  test('answers the AUTH challenge with a bridge-signed 22242 note naming the dialled relay', async () => {
    const r = await relay(() => {});
    const w = new BuzzWriter({ relayUrl: r.url, signer, onMiss: () => {} });
    try {
      await w.ready();
      await until(() => r.frames.some((f) => f[0] === 'AUTH'));
      const auth = r.frames.find((f) => f[0] === 'AUTH')![1] as BuzzNote;
      expect(auth.kind).toBe(22242);
      expect(auth.pubkey).toBe(signer.pubkey);
      expect(auth.tags).toEqual([['relay', r.url], ['challenge', 'challenge-4965']]);
      expect(schnorr.verify(auth.sig, auth.id, auth.pubkey)).toBe(true);
    } finally { w.close(); await r.close(); }
  });

  test('negative: a note the relay never OKs is a miss naming the timeout; stray frames do not settle it', async () => {
    const r = await relay((ws, note) => {
      ws.send(JSON.stringify(['NOTICE', 'hello']));
      ws.send(JSON.stringify(['OK', 'f'.repeat(64), true, '']));
      void note;
    });
    const misses: Array<{ reason: string; alert: boolean; note: BuzzNote }> = [];
    const w = new BuzzWriter({ relayUrl: r.url, signer, okTimeoutMs: 150, onMiss: (note, reason, alert) => misses.push({ note, reason, alert }) });
    try {
      await w.ready();
      w.post({ id: 42, from: 'kade', to: 'jeff', content: '[nudge from kade | 10:00] look here' });
      await until(() => misses.length > 0);
      expect(misses).toHaveLength(1);
      expect(misses[0].reason).toBe('relay sent no OK within 150ms');
      expect(misses[0].alert).toBe(true);
      // a nudge to Jeff carries just the words
      expect(misses[0].note.content).toBe('look here');
      expect(misses[0].note.tags).toContainEqual(['msg', 'pulse-42']);
    } finally { w.close(); await r.close(); }
  });

  test('negative: after the relay drops the connection, a post is a "relay not connected" miss', async () => {
    const r = await relay(() => {});
    const misses: string[] = [];
    const w = new BuzzWriter({ relayUrl: r.url, signer, onMiss: (_n, reason) => misses.push(reason) });
    try {
      await w.ready();
      for (const c of r.wss.clients) c.terminate();
      await new Promise((res) => setTimeout(res, 100));
      w.post({ id: 7, from: 'silas', to: 'kade', content: 'are you there' });
      expect(misses).toEqual(['relay not connected']);
    } finally { w.close(); await r.close(); }
  });
});

describe('#4965 the room text and frame helpers', () => {
  test('a nudge to Jeff shows only its words; a role nudge names both ends', () => {
    expect(roomText({ id: 1, from: 'kade', to: 'jeff', content: 'hi' })).toEqual({ text: 'hi', kind: 'nudge' });
    expect(roomText({ id: 1, from: 'kade', to: 'wren', content: 'hi' })).toEqual({ text: '[nudge from kade → wren] hi', kind: 'nudge' });
  });

  test('a frame handed over as buffer fragments reads as one text', () => {
    expect(frameText([Buffer.from('["EO'), Buffer.from('SE"]')])).toBe('["EOSE"]');
  });
});

describe('#4965 the bridge signer comes from the home key file', () => {
  let home: string;
  const keyFile = () => path.join(home, '.chorus', 'identity', 'bridge', 'nostr.json');
  const writeKey = (o: unknown) => { fs.mkdirSync(path.dirname(keyFile()), { recursive: true }); fs.writeFileSync(keyFile(), JSON.stringify(o)); };
  beforeEach(() => { home = fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-key-4965-')); });
  afterEach(() => fs.rmSync(home, { recursive: true, force: true }));

  test('a valid key file gives a signer whose notes verify under its pubkey', () => {
    const seckey = bytesToHex(new Uint8Array(32).fill(9));
    const pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(seckey)));
    writeKey({ seckey, pubkey });
    const { signer: s, reason } = bridgeSignerFromHome(home);
    expect(reason).toBeUndefined();
    expect(s!.pubkey).toBe(pubkey);
    const note = signNote(s!, 1_700_000_000, 9, [['h', 'x']], 'words');
    expect(schnorr.verify(note.sig, note.id, pubkey)).toBe(true);
  });

  test('negative: no file, a short seckey, and a pubkey that does not match are each named', () => {
    expect(bridgeSignerFromHome(home)).toEqual({ reason: `no bridge key at ${keyFile()}` });
    writeKey({ seckey: 'abcd' });
    expect(bridgeSignerFromHome(home)).toEqual({ reason: `${keyFile()} has no 32-byte hex seckey` });
    writeKey({ seckey: bytesToHex(new Uint8Array(32).fill(9)), pubkey: 'b'.repeat(64) });
    expect(bridgeSignerFromHome(home)).toEqual({ reason: `${keyFile()} pubkey does not match its seckey` });
  });
});
