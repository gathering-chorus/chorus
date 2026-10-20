// @test-type: unit — a mock socket and a stub relay that routes live posts the way Buzz does; no live relay.
// @domain: messages
// @card: #4652
// @owner: wren
/**
 * #4652 reopen — after the land, prod Clearing v2 showed nothing new from 08:53 (the
 * kind 9 switch) while Buzz held every post. Measured by Silas 10-20 09:12 on the
 * live relay: a one-filter subscription (kind 9 #h) got 3 live posts in 40 s, the
 * two-filter one v2 sent (kind 9 #h + kind 1 #t) got 0. Buzz routes live posts by
 * channel, and a REQ that mixes filters is registered under none. So the room asks
 * with one filter per subscription.
 */
import { BuzzRelayClient, MockWebSocket, RelaySigner, TEAM_CHANNEL } from '../src/relay-client';
import { NostrEvent } from '../src/types';

const signer: RelaySigner = { pubkey: 'ab'.repeat(32), sign: () => 'cd'.repeat(64) };

/** Buzz's live routing as measured: only a REQ with exactly one filter is registered. */
function liveSubs(ws: MockWebSocket): Array<{ id: string; filter: Record<string, unknown> }> {
  return ws.sentMessages.map((m) => JSON.parse(m)).filter((m) => m[0] === 'REQ' && m.length === 3).map((m) => ({ id: m[1], filter: m[2] }));
}

function matches(filter: Record<string, unknown>, e: NostrEvent): boolean {
  if (!(filter.kinds as number[]).includes(e.kind)) return false;
  const h = filter['#h'] as string[] | undefined;
  const t = filter['#t'] as string[] | undefined;
  if (h && !e.tags.some((x) => x[0] === 'h' && h.includes(x[1]))) return false;
  if (t && !e.tags.some((x) => x[0] === 't' && t.includes(x[1]))) return false;
  return true;
}

/** Push a live post to every registered subscription it matches, as the relay does. */
function pushLive(ws: MockWebSocket, e: NostrEvent): void {
  for (const s of liveSubs(ws)) if (matches(s.filter, e)) ws.triggerMessage(JSON.stringify(['EVENT', s.id, e]));
}

const post = (id: string, kind: number, tags: string[][]): NostrEvent => ({ id, pubkey: 'ef'.repeat(32), created_at: 1000, kind, tags, content: id, sig: '00'.repeat(64) });

describe('#4652 the room gets live channel posts from Buzz', () => {
  test('a live kind 9 #team post and a live kind 1 note both reach the room', () => {
    const ws = new MockWebSocket();
    const got: string[] = [];
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', wsFactory: () => ws, signer, onEvent: (e) => got.push(e.id) });
    client.connect();
    ws.triggerOpen();
    pushLive(ws, post('nine', 9, [['h', TEAM_CHANNEL], ['t', 'team']]));
    pushLive(ws, post('one', 1, [['t', 'team']]));
    expect(got).toEqual(['nine', 'one']);
  });

  test('NEGATIVE PROOF: the stub relay drops a REQ that mixes two filters, as Buzz did on 10-20', () => {
    const ws = new MockWebSocket();
    ws.send(JSON.stringify(['REQ', 'mixed', { kinds: [9], '#h': [TEAM_CHANNEL] }, { kinds: [1], '#t': ['team', 'reply'] }]));
    expect(liveSubs(ws)).toEqual([]);
  });

  test('each subscription asks for one filter, under its own id', () => {
    const ws = new MockWebSocket();
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', wsFactory: () => ws, signer });
    client.connect();
    ws.triggerOpen();
    const subs = liveSubs(ws);
    expect(subs.map((s) => s.filter.kinds)).toEqual([[9], [1]]);
    expect(new Set(subs.map((s) => s.id)).size).toBe(2);
  });

  test('the counts read asks per filter too and finishes only when both have answered', async () => {
    const ws = new MockWebSocket();
    const client = new BuzzRelayClient({ relayUrl: 'ws://relay', wsFactory: () => ws, signer });
    client.connect();
    ws.triggerOpen();
    const pending = client.fetchSince(500);
    const fetchSubs = liveSubs(ws).filter((s) => s.filter.since === 500);
    expect(fetchSubs).toHaveLength(2);
    let done = false;
    void pending.then(() => { done = true; });
    pushLive(ws, post('a', 9, [['h', TEAM_CHANNEL]]));
    ws.triggerMessage(JSON.stringify(['EOSE', fetchSubs[0].id]));
    await new Promise((r) => setTimeout(r, 10));
    expect(done).toBe(false);
    pushLive(ws, post('b', 1, [['t', 'team']]));
    ws.triggerMessage(JSON.stringify(['EOSE', fetchSubs[1].id]));
    expect((await pending).map((e) => e.id).sort()).toEqual(['a', 'b']);
  });
});
