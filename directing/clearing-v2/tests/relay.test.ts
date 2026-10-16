// @test-type: unit — verifies Buzz relay client connection, subscription and offline buffering, no live services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';
import { NostrEvent } from '../src/types';

describe('Clearing v2: Buzz Relay Client & Offline Resiliency', () => {
  const dummyEvent: NostrEvent = {
    id: 'ev-001',
    pubkey: 'pub-001',
    created_at: 1786464000,
    kind: 1,
    tags: [['t', 'team']],
    content: 'Test message',
    sig: 'sig-001',
  };

  test('subscribes to team topic upon connection', () => {
    const mockWs = new MockWebSocket();
    const client = new BuzzRelayClient({
      relayUrl: 'ws://127.0.0.1:3499',
      topic: 'team',
      wsFactory: () => mockWs,
    });

    client.connect();
    mockWs.triggerOpen();

    expect(client.isConnected()).toBe(true);
    // Verified NIP-01 REQ subscription was sent
    expect(mockWs.sentMessages.length).toBe(1);
    const subFrame = JSON.parse(mockWs.sentMessages[0]);
    expect(subFrame[0]).toBe('REQ');
    expect(subFrame[2]).toMatchObject({ kinds: [1], '#t': ['team', 'reply'], limit: 200 });
  });

  test('parses inbound EVENT frames and passes to onEvent handler', () => {
    const mockWs = new MockWebSocket();
    const received: NostrEvent[] = [];

    const client = new BuzzRelayClient({
      relayUrl: 'ws://127.0.0.1:3499',
      topic: 'team',
      wsFactory: () => mockWs,
      onEvent: (ev: NostrEvent) => received.push(ev),
    });

    client.connect();
    mockWs.triggerOpen();

    // Relay sends an EVENT frame
    mockWs.triggerMessage(JSON.stringify(['EVENT', 'team-sub', dummyEvent]));

    expect(received.length).toBe(1);
    expect(received[0].id).toBe('ev-001');
    expect(received[0].content).toBe('Test message');
  });

  test('NEGATIVE PROOF: offline publish queues messages and flushes upon reconnect', () => {
    const mockWs = new MockWebSocket();
    const client = new BuzzRelayClient({
      relayUrl: 'ws://127.0.0.1:3499',
      topic: 'team',
      wsFactory: () => mockWs,
    });

    // Client is NOT connected yet
    expect(client.isConnected()).toBe(false);

    // Try to publish while offline
    const queued = client.publish(dummyEvent);
    expect(queued).toBe(true);
    expect(client.getBufferedCount()).toBe(1);
    expect(mockWs.sentMessages.length).toBe(0); // Nothing sent over wire yet

    // Now connect and trigger open
    client.connect();
    mockWs.triggerOpen();

    expect(client.isConnected()).toBe(true);
    // Buffer should be flushed
    expect(client.getBufferedCount()).toBe(0);
    // Sent: 1 subscription REQ + 1 flushed EVENT
    expect(mockWs.sentMessages.length).toBe(2);
    const eventFrame = JSON.parse(mockWs.sentMessages[1]);
    expect(eventFrame[0]).toBe('EVENT');
    expect(eventFrame[1].id).toBe('ev-001');
  });
});
