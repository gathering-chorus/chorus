// @test-type: unit — a mock socket stands in for the relay; no live relay, no real key
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 — Jeff 10-19 16:10 "yet i see nothing": the Buzz app's #team channel shows
// kind 9 posts tagged h=<channel id>, and v2 wrote kind 1 notes tagged t=team, which
// the app never shows. Measured 10-20 06:55: a kind 9 h=160e1f5d… post from a
// non-member key is accepted and lands in #team.
import { BuzzRelayClient, MockWebSocket, RelaySigner, TEAM_CHANNEL } from '../src/relay-client';

const signer: RelaySigner = { pubkey: 'ab'.repeat(32), sign: () => 'cd'.repeat(64) };
const isChannelPost = (e: { kind: number; tags: string[][] }) => e.kind === 9 && e.tags.some((t) => t[0] === 'h' && t[1] === TEAM_CHANNEL);
const reqs = (ws: MockWebSocket) => ws.sentMessages.map((m) => JSON.parse(m)).filter((m) => m[0] === 'REQ');

describe('#4652 v2 posts into the #team channel the Buzz app shows', () => {
  test('a stored copy is a kind 9 post tagged with the relay\'s #team id, keeping the room tags', () => {
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', signer });
    const copy = client.signAsReader('hello', [['t', 'team'], ['author', 'jeff']], 1000)!;
    expect(copy.kind).toBe(9);
    expect(copy.tags).toContainEqual(['h', '160e1f5d-6816-4c68-94b3-b5f1ade75565']);
    expect(copy.tags).toContainEqual(['author', 'jeff']);
    expect(isChannelPost(copy)).toBe(true);
  });

  test('NEGATIVE PROOF: a kind 1 note, or a kind 9 without the h tag, is not a channel post', () => {
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', signer });
    expect(isChannelPost({ ...client.signAsReader('x', [], 1)!, kind: 1 })).toBe(false);
    expect(isChannelPost({ kind: 9, tags: [['t', 'team']] })).toBe(false);
  });

  test('the room reads the channel and still reads the old kind 1 notes, so history stays', () => {
    const ws = new MockWebSocket();
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', wsFactory: () => ws, signer });
    client.connect();
    ws.triggerOpen();
    expect(reqs(ws)[0].slice(2)).toEqual(expect.arrayContaining([
      expect.objectContaining({ kinds: [9], '#h': [TEAM_CHANNEL] }),
      expect.objectContaining({ kinds: [1], '#t': ['team', 'reply'] }),
    ]));
  });

  test('the counts read covers both kinds too', () => {
    const ws = new MockWebSocket();
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', wsFactory: () => ws, signer });
    client.connect();
    ws.triggerOpen();
    void client.fetchSince(500).catch(() => undefined);
    expect(reqs(ws).pop().slice(2)).toEqual(expect.arrayContaining([
      expect.objectContaining({ kinds: [9], '#h': [TEAM_CHANNEL], since: 500 }),
      expect.objectContaining({ kinds: [1], '#t': ['team', 'reply'], since: 500 }),
    ]));
  });

  test('a configured channel id wins over the default', () => {
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', signer, channel: 'other-channel' });
    expect(client.signAsReader('x', [], 1)!.tags).toContainEqual(['h', 'other-channel']);
  });
});
