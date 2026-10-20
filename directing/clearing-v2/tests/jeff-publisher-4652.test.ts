// @test-type: unit — a fake relay on a local socket that enforces Buzz's AUTH rule; no live relay, no real key.
// @domain: messages
// @card: #4652
// @owner: wren
/**
 * #4652 reopen 2 — a post Jeff types in Clearing is stored in Buzz as Jeff (A2, 10-20 11:53).
 * The relay stores an event only when its pubkey is the connection's AUTH identity (#4608,
 * measured 10-18), so the post goes over its own connection, AUTHed with an AUTH event
 * the signing service signs as Jeff. If the service does not sign, the post is not sent,
 * and no copy goes out under the room's own key.
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { JeffPublisher } from '../src/jeff-publisher';
import { SignResult, UnsignedEvent } from '../src/remote-signer';
import { buildEventId, serializeEvent } from '../src/identity';
import { NostrEvent } from '../src/types';

const jeffSk = new Uint8Array(32).fill(61);
const jeffPk = bytesToHex(schnorr.getPublicKey(jeffSk));

function signAsJeff(u: UnsignedEvent): NostrEvent {
  const id = buildEventId(serializeEvent(jeffPk, u.created_at, u.kind, u.tags, u.content));
  return { ...u, id, pubkey: jeffPk, sig: bytesToHex(schnorr.sign(hexToBytes(id), jeffSk)) };
}
const workingSigner = { asked: [] as number[], async signEvent(u: UnsignedEvent): Promise<SignResult> { this.asked.push(u.kind); return { ok: true, event: signAsJeff(u) }; } };
const downSigner = { async signEvent(): Promise<SignResult> { return { ok: false, reason: 'signing service did not answer within 8000 ms' }; } };

/** Buzz's rule: challenge on connect; store an event only if its pubkey is the AUTHed one. */
async function fakeRelay() {
  const stored: NostrEvent[] = [];
  const authed: string[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => {
    let who = '';
    ws.send(JSON.stringify(['AUTH', 'chal-1']));
    ws.on('message', (data) => {
      const f = JSON.parse(String(data));
      const ev = f[1] as NostrEvent;
      if (f[0] === 'AUTH') {
        const good = ev.kind === 22242 && ev.tags.some((t) => t[0] === 'challenge' && t[1] === 'chal-1');
        if (good) { who = ev.pubkey; authed.push(who); }
        ws.send(JSON.stringify(['OK', ev.id, good, good ? '' : 'auth-required: bad challenge']));
      } else if (f[0] === 'EVENT') {
        const ok = !!who && ev.pubkey === who;
        if (ok) stored.push(ev);
        ws.send(JSON.stringify(['OK', ev.id, ok, ok ? '' : 'invalid: event pubkey does not match authenticated identity']));
      }
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  return { url, stored, authed, close: () => new Promise<void>((r) => wss.close(() => server.close(() => r()))) };
}

const post: UnsignedEvent = { kind: 9, created_at: 1_800_000_000, tags: [['h', '160e1f5d-6816-4c68-94b3-b5f1ade75565'], ['t', 'team']], content: '@kade hello from Clearing' };

describe('#4652 Jeff\'s Clearing post is stored in Buzz as Jeff', () => {
  test('the post is stored signed by Jeff, over a connection AUTHed as Jeff', async () => {
    const relay = await fakeRelay();
    try {
      const r = await new JeffPublisher({ relayUrl: relay.url, signer: workingSigner }).publish(post);
      expect(r).toEqual(expect.objectContaining({ ok: true }));
      expect(relay.authed).toEqual([jeffPk]);
      expect(relay.stored.map((e) => [e.pubkey, e.content])).toEqual([[jeffPk, post.content]]);
      expect(workingSigner.asked).toEqual([9, 22242]);
    } finally { await relay.close(); }
  });

  test('NEGATIVE PROOF: the signing service is down, so nothing is stored and the reason comes back', async () => {
    const relay = await fakeRelay();
    try {
      const r = await new JeffPublisher({ relayUrl: relay.url, signer: downSigner }).publish(post);
      expect(r).toEqual({ ok: false, reason: expect.stringContaining('did not answer') });
      expect(relay.stored).toEqual([]);
    } finally { await relay.close(); }
  });

  test('NEGATIVE PROOF: the relay refusing the post is reported, never "stored"', async () => {
    const relay = await fakeRelay();
    const halfSigner = { async signEvent(u: UnsignedEvent): Promise<SignResult> { return u.kind === 22242 ? { ok: false, reason: 'only kind 9 is signed' } : { ok: true, event: signAsJeff(u) }; } };
    try {
      const r = await new JeffPublisher({ relayUrl: relay.url, signer: halfSigner }).publish(post);
      expect(r.ok).toBe(false);
      expect(relay.stored).toEqual([]);
    } finally { await relay.close(); }
  });
});
