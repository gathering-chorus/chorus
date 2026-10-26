// @test-type: integration — verifies room history fall-through to relay for rooms and pulse for direct threads
// @card: #4749
// @domain: messages

import http from 'http';
import { createApp } from '../src/server';
import { MockWebSocket } from '../src/relay-client';

describe('#4749 room history fall-through', () => {
  let mockPulse: http.Server;
  let pulsePort: number;
  let pulseRequests: string[] = [];

  beforeAll(async () => {
    mockPulse = http.createServer((req, res) => {
      pulseRequests.push(req.url || '');
      if (req.url?.startsWith('/api/messages')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify([
          { id: 'p_old_1', from: 'jeff', to: 'wren', text: 'direct message from pulse', created_at: '2026-10-26T08:00:00.000Z' }
        ]));
        return;
      }
      res.writeHead(404);
      res.end();
    });
    await new Promise<void>((resolve) => mockPulse.listen(0, '127.0.0.1', () => resolve()));
    pulsePort = (mockPulse.address() as any).port;
  });

  afterAll(async () => {
    await new Promise<void>((resolve) => mockPulse.close(() => resolve()));
  });

  beforeEach(() => {
    pulseRequests = [];
  });

  test('router buffer is read first if anchor is found in buffer', async () => {
    const { app, router, relay } = createApp({ relayUrl: '', pulseUrl: `http://127.0.0.1:${pulsePort}` });
    for (let i = 1; i <= 5; i++) {
      router.ingest({
        id: `buf_${i}`,
        from: 'wren',
        text: `buffer ${i}`,
        ts: new Date(1760800000000 + i * 1000).toISOString(),
        type: 'role-response',
        visible: true,
        room: 'team'
      });
    }

    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/rooms/team/history?before=buf_4`);
      expect(res.status).toBe(200);
      const data = await res.json() as any;
      expect(data.count).toBe(3); // buf_1, buf_2, buf_3
      expect(data.html).toContain('buffer 1');
      expect(data.html).toContain('buffer 3');
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('falls through to relay fetchBefore for room history when anchor ts provided', async () => {
    const { app, router, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: `http://127.0.0.1:${pulsePort}` });

    // Mock the WebSocket on the relay client
    const mockWs = new MockWebSocket();
    (relay as any).ws = mockWs;
    (relay as any).connected = true;
    mockWs.on('message', (data: unknown) => {
      (relay as any).handleMessage(String(data));
    });

    // Simulate answering REQ fetch with older messages (both filters send REQ)
    mockWs.send = (data: string) => {
      mockWs.sentMessages.push(data);
      const frame = JSON.parse(data);
      if (frame[0] === 'REQ') {
        const subId = frame[1];
        // Send back an older note
        const olderEvent = {
          id: 'relay_old_1',
          pubkey: 'pub_kade',
          created_at: 1760700000,
          kind: 9,
          tags: [['t', 'team'], ['room', 'team'], ['author', 'kade']],
          content: 'ancient team relay note',
          sig: 'sig1'
        };
        setTimeout(() => {
          mockWs.triggerMessage(JSON.stringify(['EVENT', subId, olderEvent]));
          mockWs.triggerMessage(JSON.stringify(['EOSE', subId]));
        }, 10);
      }
    };

    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      // Buffer does NOT contain relay_anchor_id, but client supplies ts
      const res = await fetch(`http://127.0.0.1:${port}/api/rooms/team/history?before=relay_anchor_id&ts=1760700005`);
      expect(res.status).toBe(200);
      const data = await res.json() as any;
      expect(data.count).toBe(1);
      expect(data.html).toContain('ancient team relay note');

      // VERIFY KADE'S INVARIANT: router buffer size must NOT be mutated by history read-through!
      expect(router.getRecent(100, 'team').some((m) => m.id === 'relay_old_1')).toBe(false);
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('falls through to pulse GET /api/messages for direct thread when buffer is empty', async () => {
    const { app, router, relay } = createApp({ relayUrl: '', pulseUrl: `http://127.0.0.1:${pulsePort}` });

    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      // Room is @wren (direct thread)
      const res = await fetch(`http://127.0.0.1:${port}/api/rooms/wren/history?before=p_anchor&ts=1760700000`);
      expect(res.status).toBe(200);
      const data = await res.json() as any;
      expect(data.count).toBe(1);
      expect(data.html).toContain('direct message from pulse');

      expect(pulseRequests.some((url) => url.includes('/api/messages') && (url.includes('pair=jeff%2Cwren') || url.includes('pair=jeff,wren')))).toBe(true);
      // Router buffer must remain unchanged
      expect(router.getRecent(100, 'wren')).toHaveLength(0);
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('NEGATIVE PROOF: without ts parameter, unknown anchor returns count 0', async () => {
    const { app, relay } = createApp({ relayUrl: 'ws://127.0.0.1:1', pulseUrl: `http://127.0.0.1:${pulsePort}` });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/rooms/team/history?before=non_existent_id`);
      expect(res.status).toBe(200);
      const data = await res.json() as any;
      expect(data.count).toBe(0);
      expect(data.html).toBe('');
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('NEGATIVE PROOF: unreachable relay/pulse returns 502 error instead of false empty start of history', async () => {
    // Unreachable pulse
    const { app, relay } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:54321' });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/rooms/wren/history?before=anchor&ts=1760700000`);
      expect(res.status).toBe(502);
      const data = await res.json() as any;
      expect(data.ok).toBe(false);
      expect(data.error).toBeDefined();
    } finally {
      relay.close();
      await new Promise<void>((r) => server.close(() => r()));
    }
  });
});
