// @test-type: integration — supertest drives createApp() against a stub relay on a local socket; no live relay.
// @domain: messages
// @card: #4628
// @owner: silas
/**
 * #4628 — what Jeff types in a role's terminal reaches Buzz once, written by
 * pulse. Measured 10-19: of 469 Jeff messages in Buzz that day, 393 came only
 * through Clearing v1's transcript tailer, which also wrote all 110 doubles.
 * The UserPromptSubmit hook posts the typed text here; pulse records it in Buzz
 * and delivers nothing (the role already has the words on screen).
 */
import request from 'supertest';
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from './service';
import { MessageStore } from './store';
import { BuzzWriter, type BuzzNote } from './buzz-writer';

const PEERS = ['abby-normal', 'jeff', 'kade', 'silas', 'wren'];
let peersDown = false;
jest.mock('./peers', () => ({ fetchPeers: () => (peersDown ? Promise.reject(new Error('the roles door answered HTTP 503')) : Promise.resolve(PEERS)) }));

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

describe('#4628 pulse records what Jeff types in a terminal', () => {
  beforeEach(() => { process.env.PULSE_ALLOW_DIRECT_POST = '1'; });
  afterEach(() => { delete process.env.PULSE_ALLOW_DIRECT_POST; });

  test('a typed prompt becomes one bridge note: Jeff as author, the pane it was typed in, surface terminal', async () => {
    const relay = await stubRelay();
    const { app, writer, store } = await rig(relay);
    try {
      const res = await request(app).post(ROUTE).send({ kind: 'jeff', to: 'silas', content: 'ok router is in living room' });
      expect(res.status).toBe(200);
      await until(() => relay.got.length > 0);
      expect(relay.got).toHaveLength(1);
      expect(relay.got[0].content).toBe('ok router is in living room');
      expect(relay.got[0].tags).toEqual(expect.arrayContaining([
        ['t', 'team'], ['author', 'jeff'], ['kind', 'jeff'], ['to', 'silas'], ['surface', 'terminal'],
      ]));
      // record only: the role already has the words on screen, nothing is queued for delivery
      expect(store.getPendingDeliveries()).toHaveLength(0);
    } finally { writer.close(); await relay.close(); }
  });

  test('the same words posted twice in a row (a hook retry) are one note', async () => {
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      await request(app).post(ROUTE).send({ kind: 'jeff', to: 'wren', content: 'go' });
      await request(app).post(ROUTE).send({ kind: 'jeff', to: 'wren', content: 'go' });
      await until(() => relay.got.length > 0);
      await settle();
      expect(relay.got).toHaveLength(1);
    } finally { writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF: a Clearing message delivered into the terminal arrives as a
  // prompt too; recording it would be a second copy of what v2 already wrote.
  test('negative proof: words Jeff sent from Clearing to this role are not recorded again; the same words to another role are', async () => {
    const relay = await stubRelay();
    const { app, writer, store } = await rig(relay);
    try {
      store.sendJeffInput('wren', '@wren go');
      const res = await request(app).post(ROUTE).send({ kind: 'jeff', to: 'wren', content: '@wren go' });
      expect(res.body.deduped).toBe('clearing');
      await request(app).post(ROUTE).send({ kind: 'jeff', to: 'kade', content: '@wren go' });
      await until(() => relay.got.length > 0);
      await settle();
      expect(relay.got.map((n) => n.tags.find((t) => t[0] === 'to')?.[1])).toEqual(['kade']);
    } finally { writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF (reopen 1): measured 10-19 18:56 and 19:27 — a headless session
  // with no role posted its JSON prompt as pane "unknown" and Buzz showed it as Jeff.
  // Only a role pane the roles door lists records; Jeff himself is not a pane.
  test('negative proof: a pane the roles door does not list, or jeff, is refused and writes nothing; a listed role pane records', async () => {
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      const unknown = await request(app).post(ROUTE).send({ kind: 'jeff', to: 'unknown', content: '{"timestamp": "2026-10-19T22:56:26Z"}' });
      expect(unknown.status).toBe(422);
      expect(unknown.body.error).toBe('not-a-role-pane');
      expect((await request(app).post(ROUTE).send({ kind: 'jeff', to: 'jeff', content: 'x' })).status).toBe(422);
      const ok = await request(app).post(ROUTE).send({ kind: 'jeff', to: 'kade', content: 'typed in kade' });
      expect(ok.status).toBe(200);
      await until(() => relay.got.length > 0);
      await settle();
      expect(relay.got.map((n) => n.content)).toEqual(['typed in kade']);
    } finally { writer.close(); await relay.close(); }
  });

  test('a roles door that does not answer keeps Jeff\'s words from a named pane, and still refuses unknown', async () => {
    peersDown = true;
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      expect((await request(app).post(ROUTE).send({ kind: 'jeff', to: 'silas', content: 'door down' })).status).toBe(200);
      // but a pane that is never a role is refused even with the door down (Wren 20:06)
      expect((await request(app).post(ROUTE).send({ kind: 'jeff', to: 'unknown', content: '{"timestamp": "x"}' })).status).toBe(422);
      await until(() => relay.got.length > 0);
      expect(relay.got.map((n) => n.content)).toEqual(['door down']);
    } finally { peersDown = false; writer.close(); await relay.close(); }
  });

  // NEGATIVE PROOF: a caller without the pulse secret writes nothing.
  test('negative proof: a caller without the secret is refused and nothing reaches Buzz; the hook, with it, is let in', async () => {
    delete process.env.PULSE_ALLOW_DIRECT_POST;
    process.env.CHORUS_PULSE_SECRET = 'secret-4628';
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      const res = await request(app).post(ROUTE).send({ kind: 'jeff', to: 'silas', content: 'spoofed' });
      expect(res.status).toBe(403);
      await settle();
      expect(relay.got).toHaveLength(0);
      const ok = await request(app).post(ROUTE).set('X-Chorus-Pulse-Secret', 'secret-4628').send({ kind: 'jeff', to: 'silas', content: 'typed' });
      expect(ok.status).toBe(200);
      await until(() => relay.got.length > 0);
      expect(relay.got.map((n) => n.content)).toEqual(['typed']);
    } finally { delete process.env.CHORUS_PULSE_SECRET; writer.close(); await relay.close(); }
  });

  test('a body without the pane or the words is refused', async () => {
    const relay = await stubRelay();
    const { app, writer } = await rig(relay);
    try {
      expect((await request(app).post(ROUTE).send({ kind: 'jeff', content: 'x' })).status).toBe(400);
      expect((await request(app).post(ROUTE).send({ kind: 'jeff', to: 'silas' })).status).toBe(400);
    } finally { writer.close(); await relay.close(); }
  });
});
