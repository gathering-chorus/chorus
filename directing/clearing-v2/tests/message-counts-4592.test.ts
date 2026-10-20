// @test-type: unit — the relay's fetchSince over a mock socket; no live relay.
// @card: #4592
// @owner: silas
// @domain: messages
/**
 * #4592 — the relay is asked for a window of notes separately from the room.
 * The per-kind counting moved to pulse in #4634 (message-counts.ts and its
 * tests there); /counts now presents the sensors' readings (readings-4634).
 */
import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';

describe('#4592 the relay is asked for today, separately from the room', () => {
  const ev = (id: string) => ({ id, pubkey: 'p', created_at: 1, kind: 1, tags: [['t', 'team']], content: id, sig: 's' });

  test('fetchSince collects its own subscription until EOSE, and the room never sees those events', async () => {
    const ws = new MockWebSocket();
    const roomGot: string[] = [];
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws, onEvent: (e) => roomGot.push(e.id) });
    relay.connect();
    ws.triggerOpen();
    const pending = relay.fetchSince(1000);
    const req = JSON.parse(ws.sentMessages[ws.sentMessages.length - 1]);
    expect(req[0]).toBe('REQ');
    expect(req[2]).toEqual(expect.objectContaining({ since: 1000, kinds: [9] }));
    expect(req[3]).toEqual(expect.objectContaining({ since: 1000, kinds: [1] })); // #4652 pre-switch history
    ws.triggerMessage(JSON.stringify(['EVENT', req[1], ev('a')]));
    ws.triggerMessage(JSON.stringify(['EVENT', req[1], ev('b')]));
    ws.triggerMessage(JSON.stringify(['EOSE', req[1]]));
    expect((await pending).map((e) => e.id)).toEqual(['a', 'b']);
    expect(roomGot).toEqual([]);
    expect(JSON.parse(ws.sentMessages[ws.sentMessages.length - 1])).toEqual(['CLOSE', req[1]]);
  });

  // NEGATIVE PROOF: a relay that never finishes is a refusal with a reason,
  // never an empty list that would read as "nothing reached Buzz".
  test('negative proof: no EOSE in time rejects, naming why', async () => {
    const ws = new MockWebSocket();
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws });
    relay.connect();
    ws.triggerOpen();
    await expect(relay.fetchSince(1000, 20)).rejects.toThrow(/EOSE/);
  });
});

