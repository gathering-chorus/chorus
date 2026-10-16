// @test-type: unit
// @domain: messages
// #4530 part 1 — the room reads #team only after answering the relay's NIP-42 AUTH
// with its reader key (Silas 2026-10-15 13:41). Without a key it names the gap.
import { BuzzRelayClient, MockWebSocket, RelaySigner } from '../src/relay-client';
import { buildEventId } from '../src/identity';

const signer: RelaySigner = { pubkey: 'a'.repeat(64), sign: (id) => 's'.repeat(64) + id.slice(0, 64) };

function client(withSigner: boolean) {
  const ws = new MockWebSocket();
  const c = new BuzzRelayClient({ relayUrl: 'ws://relay.test:3000', wsFactory: () => ws, signer: withSigner ? signer : undefined, nowSec: () => 1700000000 });
  c.connect();
  ws.triggerOpen();
  ws.triggerMessage(JSON.stringify(['AUTH', 'chal-1']));
  return { c, ws };
}

test('answers the challenge with a signed kind 22242 event, then asks for #team again', () => {
  const { c, ws } = client(true);
  const frames = ws.sentMessages.map((m) => JSON.parse(m));
  const authAt = frames.findIndex((f) => f[0] === 'AUTH');
  expect(authAt).toBeGreaterThan(0); // after the first REQ
  const ev = frames[authAt][1];
  expect(ev.kind).toBe(22242);
  expect(ev.tags).toEqual([['relay', 'ws://relay.test:3000'], ['challenge', 'chal-1']]);
  expect(ev.id).toBe(buildEventId(JSON.stringify([0, signer.pubkey, 1700000000, 22242, ev.tags, ''])));
  expect(frames.slice(authAt + 1).some((f) => f[0] === 'REQ')).toBe(true);
  expect(c.getAuthState()).toBe('sent');
});

test('negative proof: with no reader key it sends no AUTH and names the gap', () => {
  const { c, ws } = client(false);
  expect(ws.sentMessages.some((m) => JSON.parse(m)[0] === 'AUTH')).toBe(false);
  expect(c.getAuthState()).toBe('no-signer');
});
