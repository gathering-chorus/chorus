// @test-type: unit — a fake relay on a local socket and a stand-in publisher; no live relay, no real key.
// @domain: messages
// @card: #4652
// @owner: wren
/**
 * #4652 reopen 2 — with Jeff's signing service set up, a post he types in Clearing is
 * stored in Buzz as Jeff, and the room's own key never stores a copy. If the service
 * does not sign, Jeff is told "not stored", with the reason (Silas 11:56: no bridge copy).
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from '../src/server';
import { PublishResult } from '../src/jeff-publisher';
import { UnsignedEvent } from '../src/remote-signer';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const browserSk = new Uint8Array(32).fill(71);
const browserPk = bytesToHex(schnorr.getPublicKey(browserSk));
const bridgeSk = new Uint8Array(32).fill(72);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(bridgeSk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), bridgeSk)) };

function browserNote(content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['room', 'team']];
  const id = buildEventId(serializeEvent(browserPk, created_at, 1, tags, content));
  return { id, pubkey: browserPk, created_at, kind: 1, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), browserSk)) };
}

/** A relay that records every event the room's own connection sends. */
async function fakeRelay() {
  const fromRoom: NostrEvent[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (data) => {
    const f = JSON.parse(String(data));
    if (f[0] !== 'EVENT') return;
    fromRoom.push(f[1]);
    ws.send(JSON.stringify(['OK', f[1].id, true, '']));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `ws://127.0.0.1:${(server.address() as { port: number }).port}`, fromRoom, close: () => new Promise<void>((r) => wss.close(() => server.close(() => r()))) };
}

async function start(relayUrl: string, jeffPublisher: { asked: UnsignedEvent[]; publish(u: UnsignedEvent): Promise<PublishResult> }) {
  const created = createApp({ relayUrl, pulseUrl: 'http://127.0.0.1:9', knownKeys: new Map([[browserPk, 'jeff']]), relaySigner: bridge, jeffPublisher });
  created.relay.connect();
  for (let i = 0; i < 50 && !created.relay.isConnected(); i++) await new Promise((r) => setTimeout(r, 20));
  const server = http.createServer(created.app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  const send = async (ev: NostrEvent) => (await fetch(`http://127.0.0.1:${port}/api/send`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ event: ev }),
  })).json() as Promise<{ ok: boolean; stored?: boolean; reason?: string }>;
  const stop = async () => { created.relay.close(); await new Promise<void>((r) => server.close(() => r())); };
  return { send, stop };
}

const publisher = (result: (u: UnsignedEvent) => PublishResult) => ({ asked: [] as UnsignedEvent[], async publish(u: UnsignedEvent) { this.asked.push(u); return result(u); } });

describe('#4652 Jeff\'s Clearing post goes to Buzz as Jeff, never as a room copy', () => {
  test('the publisher is asked for a kind 9 #team post with his words, and the room sends no copy', async () => {
    const relay = await fakeRelay();
    const pub = publisher((u) => ({ ok: true, event: { ...u, id: 'f'.repeat(64), pubkey: 'a'.repeat(64), sig: '0'.repeat(128) } }));
    const v2 = await start(relay.url, pub);
    try {
      const ev = browserNote('@kade hello from Clearing');
      expect(await v2.send(ev)).toEqual(expect.objectContaining({ ok: true, stored: true }));
      expect(pub.asked).toHaveLength(1);
      expect(pub.asked[0]).toEqual(expect.objectContaining({ kind: 9, content: '@kade hello from Clearing' }));
      expect(pub.asked[0].tags).toEqual(expect.arrayContaining([['h', '03e2b251-240f-47fe-89b5-fe4e7ac648fb'], ['t', 'team'], ['signed', ev.id], ['surface', 'clearing']]));
      expect(relay.fromRoom).toEqual([]);
    } finally { await v2.stop(); await relay.close(); }
  });

  test('NEGATIVE PROOF: the signing service down means "not stored" with its reason, and still no room copy', async () => {
    const relay = await fakeRelay();
    const v2 = await start(relay.url, publisher(() => ({ ok: false, reason: 'signing service did not answer within 8000 ms' })));
    try {
      const r = await v2.send(browserNote('this one is not signed'));
      expect(r).toEqual(expect.objectContaining({ ok: true, stored: false, reason: expect.stringContaining('did not answer') }));
      expect(relay.fromRoom).toEqual([]);
    } finally { await v2.stop(); await relay.close(); }
  });
});
