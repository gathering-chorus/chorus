// @test-type: unit — a mock socket stands in for the relay; no network, no real key.
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 reopen 2 — the room's line to the signing service signs in with the room's own client key.
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex } from '@noble/hashes/utils';
import { RelaySignerTransport } from '../src/signer-transport';
import { MockWebSocket } from '../src/relay-client';
import { NostrEvent } from '../src/types';

const CLIENT = '0b'.repeat(32);
const clientPub = bytesToHex(schnorr.getPublicKey(CLIENT));
const sent = (ws: MockWebSocket) => ws.sentMessages.map((m) => JSON.parse(m));
const tick = () => new Promise((r) => setTimeout(r, 0));

describe('#4652 the signer transport', () => {
  test('signs in as the room\'s client key, then publishes and reads the OK', async () => {
    const ws = new MockWebSocket();
    const t = new RelaySignerTransport({ relayUrl: 'ws://relay', hostHeader: '192.168.86.242:3000', clientSecret: CLIENT, wsFactory: () => ws });
    const ev = { id: 'e1' } as NostrEvent;
    const done = t.publish(ev);
    ws.triggerMessage(JSON.stringify(['AUTH', 'chal']));
    const auth = sent(ws).find((m) => m[0] === 'AUTH')[1];
    expect(auth.pubkey).toBe(clientPub);
    expect(auth.tags).toEqual([['relay', 'ws://192.168.86.242:3000'], ['challenge', 'chal']]);
    ws.triggerMessage(JSON.stringify(['OK', auth.id, true, '']));
    await tick();
    expect(sent(ws).find((m) => m[0] === 'EVENT')[1].id).toBe('e1');
    ws.triggerMessage(JSON.stringify(['OK', 'e1', true, '']));
    expect(await done).toEqual({ ok: true, reason: '' });
  });

  test('NEGATIVE PROOF: a refused sign-in is reported and nothing is published', async () => {
    const ws = new MockWebSocket();
    const t = new RelaySignerTransport({ relayUrl: 'ws://relay', clientSecret: CLIENT, wsFactory: () => ws });
    const done = t.publish({ id: 'e2' } as NostrEvent);
    ws.triggerMessage(JSON.stringify(['AUTH', 'chal']));
    const auth = sent(ws).find((m) => m[0] === 'AUTH')[1];
    ws.triggerMessage(JSON.stringify(['OK', auth.id, false, 'auth-required: no']));
    expect(await done).toEqual({ ok: false, reason: expect.stringContaining('refused the room') });
    expect(sent(ws).some((m) => m[0] === 'EVENT')).toBe(false);
  });

  test('events on a subscription reach only that subscription', async () => {
    const ws = new MockWebSocket();
    const t = new RelaySignerTransport({ relayUrl: 'ws://relay', clientSecret: CLIENT, wsFactory: () => ws });
    const got: string[] = [];
    t.subscribe({ kinds: [24133] }, (e) => got.push(e.id));
    ws.triggerMessage(JSON.stringify(['AUTH', 'chal']));
    ws.triggerMessage(JSON.stringify(['OK', sent(ws).find((m) => m[0] === 'AUTH')[1].id, true, '']));
    await tick();
    const subId = sent(ws).find((m) => m[0] === 'REQ')[1];
    ws.triggerMessage(JSON.stringify(['EVENT', subId, { id: 'mine' }]));
    ws.triggerMessage(JSON.stringify(['EVENT', 'someone-else', { id: 'not-mine' }]));
    expect(got).toEqual(['mine']);
  });
});
