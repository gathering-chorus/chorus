// @test-type: unit — relay client fetchBefore over mock socket
// @card: #4749
// @owner: abby-normal
// @domain: messages
import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';

describe('#4749 BuzzRelayClient.fetchBefore', () => {
  const ev = (id: string, created_at: number) => ({
    id,
    pubkey: 'p',
    created_at,
    kind: 9,
    tags: [['t', 'team']],
    content: `msg-${id}`,
    sig: 's',
  });

  test('fetches older notes with until filter, dedupes, excludes anchorId, sorts newest-first', async () => {
    const ws = new MockWebSocket();
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws });
    relay.connect();
    ws.triggerOpen();

    const pending = (relay as any).fetchBefore(1500, 2, 'anchor');
    const [req9, req1] = ws.sentMessages.slice(-2).map((m) => JSON.parse(m));
    expect(req9[0]).toBe('REQ');
    expect(req9[2]).toEqual(expect.objectContaining({ until: 1500, limit: 2 }));
    expect(req1[2]).toEqual(expect.objectContaining({ until: 1500, limit: 2 }));

    ws.triggerMessage(JSON.stringify(['EVENT', req9[1], ev('anchor', 1500)]));
    ws.triggerMessage(JSON.stringify(['EVENT', req9[1], ev('older1', 1400)]));
    ws.triggerMessage(JSON.stringify(['EVENT', req1[1], ev('older2', 1300)]));
    ws.triggerMessage(JSON.stringify(['EVENT', req1[1], ev('older3', 1200)]));
    ws.triggerMessage(JSON.stringify(['EOSE', req9[1]]));
    ws.triggerMessage(JSON.stringify(['EOSE', req1[1]]));

    const res = await pending;
    expect(res.map((e: any) => e.id)).toEqual(['older1', 'older2']);
  });

  test('NEGATIVE PROOF: timeout without EOSE rejects', async () => {
    const ws = new MockWebSocket();
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws });
    relay.connect();
    ws.triggerOpen();
    await expect((relay as any).fetchBefore(1500, 10, undefined, 20)).rejects.toThrow(/EOSE/);
  });
});
