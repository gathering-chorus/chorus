// @test-type: unit — a stub key registry and a stub pulse on local ports; no live door, no live relay.
// @card: #4652
// @owner: wren
// @domain: messages
/**
 * #4652 — the room learns whose key is whose from the security graph's key
 * registry, never from local files. Measured 10-20 07:43: Jeff's local
 * ~/.chorus/identity/jeff/nostr.json held ee70fd19, a key bound to no one, while
 * the registry binds his Buzz app key a42ccebe to principal-jeff. Reading files,
 * the room would have treated his app posts as unattributed and trusted a stale key.
 * Silas 07:44: graph only; a local file must never name anyone.
 */
import fs from 'fs';
import http from 'http';
import os from 'os';
import path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { keysFromRegistry, RegistryKeys, NOSTR_TYPE } from '../src/key-registry';
import { makeInboundHandler, createApp } from '../src/server';
import { MessageRouter } from '../src/router';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const staleSk = new Uint8Array(32).fill(91); // the stale local-file key
const appSk = new Uint8Array(32).fill(92); // the registry's key for Jeff
const stalePk = bytesToHex(schnorr.getPublicKey(staleSk));
const appPk = bytesToHex(schnorr.getPublicKey(appSk));

const row = (principal: string, pubkey: string) => ({ name: `cred-${principal}-nostr`, forPrincipal: `principal-${principal}`, keyType: NOSTR_TYPE, nostrPubkey: pubkey });

function post(sk: Uint8Array, content: string): NostrEvent {
  const pubkey = bytesToHex(schnorr.getPublicKey(sk));
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['h', '160e1f5d-6816-4c68-94b3-b5f1ade75565']];
  const id = buildEventId(serializeEvent(pubkey, created_at, 9, tags, content));
  return { id, pubkey, created_at, kind: 9, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), sk)) };
}

async function serve(handler: http.RequestListener): Promise<{ url: string; close: () => Promise<void> }> {
  const server = http.createServer(handler);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, close: () => new Promise<void>((r) => server.close(() => r())) };
}

async function stubPulse() {
  const got: string[] = [];
  const s = await serve((req, res) => { let b = ''; req.on('data', (c) => { b += c; }); req.on('end', () => { got.push(b); res.writeHead(202).end(); }); });
  return { ...s, got };
}

const settle = (ms = 300) => new Promise((r) => setTimeout(r, ms));

describe('#4652 keys come from the registry rows', () => {
  test('nostr rows map pubkey to the principal\'s name', () => {
    const keys = keysFromRegistry({ data: [row('jeff', appPk.toUpperCase()), row('bridge', 'f3'.repeat(32))] });
    expect(keys.get(appPk)).toBe('jeff');
    expect(keys.get('f3'.repeat(32))).toBe('bridge');
  });

  test('NEGATIVE PROOF: rows that are not nostr keys, or are malformed, name no one', () => {
    const keys = keysFromRegistry({ data: [
      { ...row('kade', 'de'.repeat(32)), keyType: 'css-client' },
      row('wren', 'not-hex'),
      { ...row('abby', 'ab'.repeat(32)), forPrincipal: 'someone-else' },
    ] });
    expect(keys.size).toBe(0);
  });
});

describe('#4652 a local file never names anyone (Silas 07:44)', () => {
  test('NEGATIVE PROOF: a local jeff/nostr.json says ee70; the registry says a42c; only a42c is Jeff', async () => {
    const home = fs.mkdtempSync(path.join(os.tmpdir(), 'keys-4652-'));
    fs.mkdirSync(path.join(home, '.chorus', 'identity', 'jeff'), { recursive: true });
    fs.writeFileSync(path.join(home, '.chorus', 'identity', 'jeff', 'nostr.json'), JSON.stringify({ pubkey: stalePk }));
    const door = await serve((_req, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ data: [row('jeff', appPk)] })); });
    const pulse = await stubPulse();
    const prevHome = process.env.HOME;
    process.env.HOME = home;
    try {
      const keys = new Map<string, string>();
      const registry = new RegistryKeys(keys, door.url);
      await registry.start();
      registry.stop();
      const handler = makeInboundHandler({ router: new MessageRouter(), keys, pulseUrl: pulse.url, broadcastSSE: () => undefined, isRelayConnected: () => true });
      handler(post(staleSk, '@silas from the stale key'));
      handler(post(appSk, '@kade from the app'));
      await settle();
      expect(pulse.got.map((b) => JSON.parse(b).to)).toEqual(['kade']);
      expect(keys.has(stalePk)).toBe(false);
    } finally {
      process.env.HOME = prevHome;
      await door.close();
      await pulse.close();
    }
  });
});

describe('#4652 the door down at start fails closed, then a retry loads the keys', () => {
  test('NEGATIVE PROOF: with the door down nothing is handed off; once it answers, the next post is', async () => {
    let up = false;
    const door = await serve((_req, res) => {
      if (!up) { res.writeHead(503).end(); return; }
      res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ data: [row('jeff', appPk)] }));
    });
    const pulse = await stubPulse();
    const keys = new Map<string, string>();
    const registry = new RegistryKeys(keys, door.url, 100);
    try {
      await registry.start();
      expect(keys.size).toBe(0);
      const handler = makeInboundHandler({ router: new MessageRouter(), keys, pulseUrl: pulse.url, broadcastSSE: () => undefined, isRelayConnected: () => true });
      handler(post(appSk, '@silas while the door is down'));
      await settle();
      expect(pulse.got).toEqual([]);
      up = true;
      await settle(400);
      expect(keys.get(appPk)).toBe('jeff');
      handler(post(appSk, '@silas once the door is back'));
      await settle();
      expect(pulse.got.map((b) => JSON.parse(b).content)).toEqual(['@silas once the door is back']);
    } finally {
      registry.stop();
      await door.close();
      await pulse.close();
    }
  });
});

describe('#4652 the registry is re-read, so a revoked key stops speaking (Silas 07:51)', () => {
  test('NEGATIVE PROOF: a row removed from the registry is dropped on the next read; a session-bound key stays', async () => {
    let rows = [row('jeff', appPk)];
    const asked: string[] = [];
    const door = await serve((req, res) => { asked.push(req.url || ''); res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ data: rows })); });
    const keys = new Map<string, string>([['b0'.repeat(32), 'jeff']]); // bound by a signed-in session
    const registry = new RegistryKeys(keys, door.url, 60000, 100);
    try {
      await registry.start();
      expect(keys.get(appPk)).toBe('jeff');
      rows = [];
      await settle(400);
      expect(keys.has(appPk)).toBe(false);
      expect(keys.get('b0'.repeat(32))).toBe('jeff');
      expect(asked[0]).toContain('limit=1000');
    } finally {
      registry.stop();
      await door.close();
    }
  });
});

describe('#4652 the room reads the registry before it reads the relay (demo 08:08)', () => {
  // Measured on demo round 747a61da: 93 of 100 replayed posts were labelled
  // "unattributed", every one older than the room's start, because the relay's
  // replay arrived before the registry answered.
  test('keysReady resolves only once the registry has filled the keys', async () => {
    let answer: () => void = () => undefined;
    const gate = new Promise<void>((r) => { answer = r; });
    const door = await serve((_req, res) => { void gate.then(() => res.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ data: [row('jeff', appPk)] }))); });
    try {
      const created = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', keysUrl: door.url });
      let ready = false;
      void created.keysReady.then(() => { ready = true; });
      await settle(200);
      expect(ready).toBe(false);
      answer();
      await created.keysReady;
      expect(resolveRole(created, appPk)).toBe('jeff');
      created.stopKeys();
    } finally {
      await door.close();
    }
  });

  test('NEGATIVE PROOF: with the door down, keysReady still resolves (the room starts, nobody attributed)', async () => {
    const door = await serve((_req, res) => { res.writeHead(503).end(); });
    try {
      const created = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', keysUrl: door.url });
      await created.keysReady;
      expect(resolveRole(created, appPk)).toBeNull();
      created.stopKeys();
    } finally {
      await door.close();
    }
  });
});

function resolveRole(created: ReturnType<typeof createApp>, pubkey: string): string | null {
  return created.keys.get(pubkey) ?? null;
}
