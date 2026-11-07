// @test-type: integration — supertest drives createApp() against a stub relay on a local socket; no live relay.
// @domain: messages
// @card: #4628
// @owner: silas
/**
 * #4628 — the land notice ("Accepted #N — title") reaches Buzz once, written
 * by pulse. Until now only Clearing v1's spine tailer posted it (13 on 10-19),
 * so stopping v1 would lose it. `cards done` posts it here; pulse records it.
 */
import request from 'supertest';
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from './service';
import { MessageStore } from './store';
import { BuzzWriter, type BuzzNote } from './buzz-writer';

jest.mock('./peers', () => ({ fetchPeers: () => Promise.resolve(['abby-normal', 'jeff', 'kade', 'silas', 'wren']) }));

const ROUTE = '/api/record';
const sk = new Uint8Array(32).fill(62);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

async function stubRelay() {
  const got: BuzzNote[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (data) => {
    const f = JSON.parse((data as Buffer).toString('utf8'));
    if (f[0] !== 'EVENT') return;
    got.push(f[1]);
    ws.send(JSON.stringify(['OK', f[1].id, true, '']));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  const close = () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); });
  return { url, got, close };
}

async function rig(relay: { url: string }) {
  const writer = new BuzzWriter({ relayUrl: relay.url, signer: bridge, okTimeoutMs: 3000, onMiss: jest.fn() });
  await writer.ready();
  const store = new MessageStore(':memory:');
  const app = createApp(store, undefined, undefined, undefined, undefined, undefined, writer);
  return { app, writer, store };
}

const until = async (cond: () => boolean, ms = 3000) => {
  for (let t = 0; t < ms && !cond(); t += 20) await new Promise((r) => setTimeout(r, 20));
};
const settle = () => new Promise((r) => setTimeout(r, 200));

describe('#4628 pulse records the land notice', () => {
  beforeEach(() => { process.env.PULSE_ALLOW_DIRECT_POST = '1'; });
  afterEach(() => { delete process.env.PULSE_ALLOW_DIRECT_POST; });

  test('a land becomes one note: the words v1 showed, the acceptor as author, kind land', async () => {
    const relay = await stubRelay();
    const { app, writer, store } = await rig(relay);
    try {
      const res = await request(app).post(ROUTE).send({ kind: 'land', card: 4630, title: 'Backups that really hold messages and streams', accepter: 'jeff', owner: 'silas' });
      expect(res.status).toBe(200);
      await until(() => relay.got.length > 0);
      expect(relay.got).toHaveLength(1);
      expect(relay.got[0].content).toBe('Accepted #4630 — Backups that really hold messages and streams');
      expect(relay.got[0].tags).toEqual(expect.arrayContaining([['t', 'team'], ['author', 'jeff'], ['kind', 'land'], ['owner', 'silas'], ['msg', 'pulse-land-4630']]));
      expect(store.getPendingDeliveries()).toHaveLength(0);
    } finally { writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF: a land that names no accepter writes no note and says so (#3743: never a default to Jeff).
  test('negative proof: no accepter is a loud refusal and no note; no title is just the number', async () => {
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      const res = await request(app).post(ROUTE).send({ kind: 'land', card: 4568, title: 'Crawl on each land' });
      expect(res.status).toBe(422);
      expect(res.body.error).toBe('no-accepter');
      await request(app).post(ROUTE).send({ kind: 'land', card: 4567, accepter: 'jeff' });
      await until(() => relay.got.length > 0);
      await settle();
      expect(relay.got.map((n) => n.content)).toEqual(['Accepted #4567']);
    } finally { writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF: the land emits its accept more than once; a second notice for the same card is dropped.
  test('negative proof: the same card twice within 10 minutes is one note; another card is its own', async () => {
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      await request(app).post(ROUTE).send({ kind: 'land', card: 4646, title: 't', accepter: 'jeff' });
      await request(app).post(ROUTE).send({ kind: 'land', card: 4646, title: 't', accepter: 'jeff' });
      await request(app).post(ROUTE).send({ kind: 'land', card: 4645, title: 'u', accepter: 'jeff' });
      await until(() => relay.got.length >= 2);
      await settle();
      expect(relay.got.map((n) => n.content)).toEqual(['Accepted #4646 — t', 'Accepted #4645 — u']);
    } finally { writer.close(); await relay.close(); }
  });

  test('a caller without the secret is refused; a body without a card number is refused', async () => {
    delete process.env.PULSE_ALLOW_DIRECT_POST;
    process.env.CHORUS_PULSE_SECRET = 'secret-4628';
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      expect((await request(app).post(ROUTE).send({ kind: 'land', card: 1, accepter: 'jeff' })).status).toBe(403);
      expect((await request(app).post(ROUTE).set('X-Chorus-Pulse-Secret', 'secret-4628').send({ kind: 'land', title: 'x', accepter: 'jeff' })).status).toBe(400);
      await settle();
      expect(relay.got).toHaveLength(0);
    } finally { delete process.env.CHORUS_PULSE_SECRET; writer.close(); await relay.close(); }
  });
});
