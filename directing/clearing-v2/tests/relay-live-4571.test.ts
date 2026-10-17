// @test-type: unit — MockWebSocket and a temp home stand in for the relay and the identity file; no live services
// @domain: messages
// @card: #4571
// @owner: wren
// #4571 — v2's Rooms read the live Buzz relay. Measured 2026-10-16 19:58: dialing the
// tunnel (127.0.0.1:13000) without the relay's own Host gets a 404; with it the relay
// challenges (NIP-42) and refuses REQ until AUTH. "Connected" must mean the room can
// read, a dropped socket must come back, and the reader key is the bridge identity v1 uses.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex } from '@noble/hashes/utils';
import { BuzzRelayClient, MockWebSocket, RelaySigner } from '../src/relay-client';
import { bridgeSigner } from '../src/relay-signer';

const signer: RelaySigner = { pubkey: 'a'.repeat(64), sign: (id) => 's'.repeat(64) + id.slice(0, 64) };

function open(opts: { hostHeader?: string; withSigner?: boolean; sockets?: MockWebSocket[] } = {}) {
  const sockets = opts.sockets ?? [];
  const c = new BuzzRelayClient({
    relayUrl: 'ws://127.0.0.1:13000', hostHeader: opts.hostHeader,
    wsFactory: () => { const w = new MockWebSocket(); sockets.push(w); return w; },
    signer: opts.withSigner === false ? undefined : signer, nowSec: () => 1700000000, reconnectDelayMs: 50,
  });
  c.connect();
  return { c, sockets };
}

test('AUTH names the relay by its own address when dialing through the tunnel', () => {
  const { sockets } = open({ hostHeader: '192.168.86.242:3000' });
  sockets[0].triggerOpen();
  sockets[0].triggerMessage(JSON.stringify(['AUTH', 'chal']));
  const auth = sockets[0].sentMessages.map((m) => JSON.parse(m)).find((f) => f[0] === 'AUTH');
  expect(auth[1].tags[0]).toEqual(['relay', 'ws://192.168.86.242:3000']);
});

test('connected only once the relay accepts the AUTH, not when the socket opens', () => {
  const { c, sockets } = open();
  sockets[0].triggerOpen();
  sockets[0].triggerMessage(JSON.stringify(['AUTH', 'chal']));
  expect(c.isConnected()).toBe(false);
  const auth = sockets[0].sentMessages.map((m) => JSON.parse(m)).find((f) => f[0] === 'AUTH');
  sockets[0].triggerMessage(JSON.stringify(['OK', auth[1].id, true, '']));
  expect(c.isConnected()).toBe(true);
});

test('negative proof: a refused AUTH or no reader key never reads as connected', () => {
  const a = open();
  a.sockets[0].triggerOpen();
  a.sockets[0].triggerMessage(JSON.stringify(['AUTH', 'chal']));
  const auth = a.sockets[0].sentMessages.map((m) => JSON.parse(m)).find((f) => f[0] === 'AUTH');
  a.sockets[0].triggerMessage(JSON.stringify(['OK', auth[1].id, false, 'restricted']));
  expect(a.c.isConnected()).toBe(false);
  const b = open({ withSigner: false });
  b.sockets[0].triggerOpen();
  b.sockets[0].triggerMessage(JSON.stringify(['AUTH', 'chal']));
  expect(b.c.isConnected()).toBe(false);
  expect(b.c.getAuthState()).toBe('no-signer');
});

test('a dropped socket reconnects; close() stays closed', () => {
  jest.useFakeTimers();
  try {
    const { c, sockets } = open();
    sockets[0].triggerOpen();
    sockets[0].triggerClose();
    expect(c.isConnected()).toBe(false);
    jest.advanceTimersByTime(60);
    expect(sockets.length).toBe(2);
    c.close();
    jest.advanceTimersByTime(10_000);
    expect(sockets.length).toBe(2);
  } finally {
    jest.useRealTimers();
  }
});

test('the reader key is the registered bridge identity; a missing file names itself', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'v2-4571-'));
  try {
    expect(bridgeSigner(home)).toEqual({ signer: undefined, reason: expect.stringContaining('identity/bridge/nostr.json') });
    const sk = schnorr.utils.randomPrivateKey();
    const pubkey = bytesToHex(schnorr.getPublicKey(sk));
    fs.mkdirSync(path.join(home, '.chorus/identity/bridge'), { recursive: true });
    fs.writeFileSync(path.join(home, '.chorus/identity/bridge/nostr.json'), JSON.stringify({ seckey: bytesToHex(sk), pubkey }));
    const { signer: s } = bridgeSigner(home);
    expect(s?.pubkey).toBe(pubkey);
    const id = 'ab'.repeat(32);
    expect(schnorr.verify(s!.sign(id), id, pubkey)).toBe(true);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

