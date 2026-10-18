// @test-type: unit — a fake relay and a fake pulse on local sockets; no live relay, no pulse.
// @card: #4625
// @owner: silas
// @domain: messages
/**
 * #4625 — a demo Clearing v2 reads the real room but never writes to it.
 *
 * Measured 2026-10-18 (Kade): v2 has two prod write paths and no switch. /api/send
 * publishes into the room and hands Jeff's words to pulse, and the inbound handler
 * hands every Jeff-signed note it reads to pulse. A demo v2 reading the real relay
 * would re-deliver Jeff's real messages to the roles. Read-only stops all three.
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp, makeInboundHandler } from '../src/server';
import { MessageRouter } from '../src/router';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const jeffSk = new Uint8Array(32).fill(61);
const jeffPk = bytesToHex(schnorr.getPublicKey(jeffSk));
const bridgeSk = new Uint8Array(32).fill(62);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(bridgeSk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), bridgeSk)) };
const keys = new Map([[jeffPk, 'jeff']]);

function jeffNote(content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['room', 'team']];
  const id = buildEventId(serializeEvent(jeffPk, created_at, 1, tags, content));
  return { id, pubkey: jeffPk, created_at, kind: 1, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), jeffSk)) };
}

async function fakeRelay() {
  const stored: NostrEvent[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (data) => {
    const f = JSON.parse(String(data));
    if (f[0] !== 'EVENT') return;
    stored.push(f[1]);
    ws.send(JSON.stringify(['OK', f[1].id, true, '']));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, stored, close: () => new Promise<void>((r) => wss.close(() => server.close(() => r()))) };
}

async function fakePulse() {
  const hits: string[] = [];
  const server = http.createServer((req, res) => { hits.push(req.url || ''); res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{"ok":true,"id":1}'); });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, hits, close: () => new Promise<void>((r) => server.close(() => r())) };
}

const settle = () => new Promise((r) => setTimeout(r, 150));

describe('#4625 a read-only v2 never writes the room or the roles', () => {
  test('/api/send answers 403 by name, and neither the room nor pulse gets a write', async () => {
    const relay = await fakeRelay();
    const pulse = await fakePulse();
    const created = createApp({ relayUrl: relay.url, pulseUrl: pulse.url, knownKeys: keys, relaySigner: bridge, readOnly: true });
    created.relay.connect();
    for (let i = 0; i < 50 && !created.relay.isConnected(); i++) await new Promise((r) => setTimeout(r, 20));
    const server = http.createServer(created.app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    try {
      const port = (server.address() as { port: number }).port;
      const res = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ event: jeffNote('@wren hello') }),
      });
      const body = await res.json() as { ok: boolean; reason?: string };
      await settle();
      expect(res.status).toBe(403);
      expect(body.reason).toBe('read-only demo: this Clearing does not send');
      expect(relay.stored).toEqual([]);
      expect(pulse.hits).toEqual([]);
    } finally {
      created.relay.close();
      await new Promise<void>((r) => server.close(() => r()));
      await relay.close();
      await pulse.close();
    }
  });

  test('NEGATIVE PROOF: with read-only off the same send writes the room and pulse, so the check above can see both', async () => {
    const relay = await fakeRelay();
    const pulse = await fakePulse();
    const created = createApp({ relayUrl: relay.url, pulseUrl: pulse.url, knownKeys: keys, relaySigner: bridge, readOnly: false });
    created.relay.connect();
    for (let i = 0; i < 50 && !created.relay.isConnected(); i++) await new Promise((r) => setTimeout(r, 20));
    const server = http.createServer(created.app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    try {
      const port = (server.address() as { port: number }).port;
      const res = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ event: jeffNote('@wren hello') }),
      });
      await settle();
      expect(res.status).toBe(200);
      expect(relay.stored.length).toBe(1);
      expect(pulse.hits.length).toBeGreaterThan(0);
    } finally {
      created.relay.close();
      await new Promise<void>((r) => server.close(() => r()));
      await relay.close();
      await pulse.close();
    }
  });

  test('a Jeff note read from the room is shown but not handed to pulse again', async () => {
    const pulse = await fakePulse();
    const router = new MessageRouter(50);
    try {
      const onEvent = makeInboundHandler({ router, keys, pulseUrl: pulse.url, broadcastSSE: () => undefined, isRelayConnected: () => true, readOnly: true });
      onEvent(jeffNote('@kade are you there'));
      await settle();
      expect(router.getRecent(10).length).toBe(1);
      expect(pulse.hits).toEqual([]);
    } finally {
      await pulse.close();
    }
  });

  test('NEGATIVE PROOF: the same note with read-only off does reach pulse, so the check above can see a write', async () => {
    const pulse = await fakePulse();
    try {
      const onEvent = makeInboundHandler({ router: new MessageRouter(50), keys, pulseUrl: pulse.url, broadcastSSE: () => undefined, isRelayConnected: () => true });
      onEvent(jeffNote('@kade are you there'));
      await settle();
      expect(pulse.hits.length).toBe(1);
    } finally {
      await pulse.close();
    }
  });
});
