// @test-type: integration — supertest drives createApp() against a stub relay on a local socket; no live relay.
// @domain: messages
// @card: #4609
// @owner: silas
/**
 * #4609 step A — pulse writes every nudge and alert to Buzz itself, in parallel
 * with delivery. Jeff 2026-10-18 11:51: "all nudges must be backed by buzz
 * right?" Measured: no; Buzz got copies only through Clearing v1's spine tailer
 * (#4592), which stops silently when v1 stops. Jeff 14:36: on relay down,
 * deliver anyway. Jeff input stays written by Clearing v2 (#4608).
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

const NUDGE = '/api/nudge';

beforeAll(() => { process.env.PULSE_ALLOW_DIRECT_POST = '1'; });
afterAll(() => { delete process.env.PULSE_ALLOW_DIRECT_POST; });

const sk = new Uint8Array(32).fill(61);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

/** A relay that answers each EVENT after `delayMs`, accepting or refusing it. */
async function stubRelay(opts: { delayMs: number; accept: boolean }) {
  const got: BuzzNote[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (data) => {
    const f = JSON.parse((data as Buffer).toString('utf8'));
    if (f[0] !== 'EVENT') return;
    got.push(f[1]);
    setTimeout(() => ws.send(JSON.stringify(['OK', f[1].id, opts.accept, opts.accept ? '' : 'invalid: event pubkey does not match authenticated identity'])), opts.delayMs);
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  const close = () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); });
  return { url, got, close };
}

async function rig(relay: { url: string }, onMiss = jest.fn()) {
  const writer = new BuzzWriter({ relayUrl: relay.url, signer: bridge, okTimeoutMs: 6000, onMiss });
  await writer.ready();
  const store = new MessageStore(':memory:');
  const app = createApp(store, undefined, undefined, undefined, undefined, undefined, writer);
  return { app, writer, onMiss };
}

const until = async (cond: () => boolean, ms = 3000) => {
  for (let t = 0; t < ms && !cond(); t += 20) await new Promise((r) => setTimeout(r, 20));
};

describe('#4609 pulse writes nudges and alerts to Buzz', () => {
  test('a nudge becomes one bridge note (author, kind, id tags) and delivery does not wait on the relay', async () => {
    const relay = await stubRelay({ delayMs: 2000, accept: true });
    const { app, writer } = await rig(relay);
    try {
      const t0 = Date.now();
      const res = await request(app).post(NUDGE).send({ from: 'kade', to: 'wren', content: 'ping' });
      expect(res.status).toBe(200);
      expect(Date.now() - t0).toBeLessThan(1000); // the relay holds its OK for 2 s
      await until(() => relay.got.length > 0);
      expect(relay.got).toHaveLength(1);
      const note = relay.got[0];
      expect(note.pubkey).toBe(bridge.pubkey);
      expect(note.content).toBe('[nudge from kade → wren] ping');
      expect(note.tags).toEqual(expect.arrayContaining([['h', '160e1f5d-6816-4c68-94b3-b5f1ade75565'], ['author', 'kade'], ['kind', 'nudge'], ['msg', `pulse-${res.body.id}`]]));
    } finally { writer.close(); await relay.close(); }
  });

  test('a system alert is posted as an alert', async () => {
    const relay = await stubRelay({ delayMs: 0, accept: true });
    const { app, writer } = await rig(relay);
    try {
      await request(app).post(NUDGE).send({ from: 'system', to: 'silas', content: 'log stale' });
      await until(() => relay.got.length > 0);
      expect(relay.got[0].content).toBe('[alert → silas] log stale');
      expect(relay.got[0].tags).toEqual(expect.arrayContaining([['author', 'system'], ['kind', 'alert']]));
    } finally { writer.close(); await relay.close(); }
  });

  test('Jeff input is not posted by pulse (Clearing v2 is its one writer, #4608)', async () => {
    const relay = await stubRelay({ delayMs: 0, accept: true });
    const { app, writer } = await rig(relay);
    try {
      await request(app).post('/api/jeff-input').send({ to: 'wren', content: 'hello' });
      await new Promise((r) => setTimeout(r, 200));
      expect(relay.got).toHaveLength(0);
    } finally { writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF: a refusing relay never stops delivery; each miss is
  // reported, and the alert fires once, not once per message.
  test('negative proof: a refusing relay still delivers, reports each miss, alerts once', async () => {
    const relay = await stubRelay({ delayMs: 0, accept: false });
    const onMiss = jest.fn();
    const { app, writer } = await rig(relay, onMiss);
    try {
      const a = await request(app).post(NUDGE).send({ from: 'kade', to: 'wren', content: 'one' });
      const b = await request(app).post(NUDGE).send({ from: 'kade', to: 'wren', content: 'two' });
      expect(a.body.ok).toBe(true);
      expect(b.body.ok).toBe(true);
      await until(() => onMiss.mock.calls.length >= 2);
      expect(onMiss).toHaveBeenCalledTimes(2);
      expect(onMiss.mock.calls[0][1]).toMatch(/does not match authenticated identity/);
      expect(onMiss.mock.calls.map((c) => c[2])).toEqual([true, false]); // alert on the first only
    } finally { writer.close(); await relay.close(); }
  });

  test('negative proof: with no relay at all, the nudge is still delivered and the miss reported', async () => {
    const onMiss = jest.fn();
    const writer = new BuzzWriter({ relayUrl: 'ws://127.0.0.1:9', signer: bridge, okTimeoutMs: 500, onMiss });
    const store = new MessageStore(':memory:');
    const app = createApp(store, undefined, undefined, undefined, undefined, undefined, writer);
    try {
      const res = await request(app).post(NUDGE).send({ from: 'kade', to: 'wren', content: 'anyone?' });
      expect(res.body.ok).toBe(true);
      await until(() => onMiss.mock.calls.length > 0);
      expect(onMiss.mock.calls[0][1]).toMatch(/not connected|no OK/);
    } finally { writer.close(); }
  });
});
