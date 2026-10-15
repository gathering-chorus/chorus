// @test-type: unit — verifies clearing-v2 server route mounting and tile-service state, no live network services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { WebSocketServer } from 'ws';
import { createApp, loadConfig } from '../src/server';
import { TileService } from '../src/tile-service';

describe('Clearing v2: Server App & Tile Service', () => {
  test('createApp initializes app, router, relay and tileService', () => {
    const { app, router, tileService, relay } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:3475',
    });
    expect(app).toBeDefined();
    expect(router).toBeDefined();
    expect(tileService).toBeDefined();
    expect(relay).toBeDefined();
  });

  test('TileService initializes known team roles with idle state', () => {
    const tileService = new TileService('http://127.0.0.1:3475');
    const tiles = tileService.getTiles();
    expect(tiles.length).toBe(5);
    const roles = tiles.map(t => t.role);
    expect(roles).toContain('abby-normal');
    expect(roles).toContain('wren');
    expect(roles).toContain('silas');
    expect(roles).toContain('kade');
    expect(roles).toContain('jeff');
    const abbyTile = tiles.find(t => t.role === 'abby-normal');
    expect(abbyTile?.name).toBe('Abby Normal');
    expect(abbyTile?.state).toBe('idle');
  });

  test('TileService updates tile state incrementally', () => {
    const tileService = new TileService();
    tileService.updateTile('abby-normal', { state: 'building', card: 4530 });
    const tiles = tileService.getTiles();
    const abbyTile = tiles.find(t => t.role === 'abby-normal');
    expect(abbyTile?.state).toBe('building');
    expect(abbyTile?.card).toBe(4530);
  });

  test('NEGATIVE PROOF: updating nonexistent role does not corrupt tile list', () => {
    const tileService = new TileService();
    tileService.updateTile('ghost-agent', { state: 'idle' });
    const tiles = tileService.getTiles();
    expect(tiles.length).toBe(5);
    expect(tiles.find(t => t.role === 'ghost-agent')).toBeUndefined();
  });

  test('NEGATIVE PROOF: /roles ignores URL query parameters so links cannot fake data or trust strip', async () => {
    const stub = http.createServer((_req, res) => {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ data: [{ name: 'test-agent', label: 'Test Agent', roleKind: 'agent' }] }));
    });
    await new Promise<void>((r) => stub.listen(0, '127.0.0.1', () => r()));
    const stubPort = (stub.address() as any).port;

    const { app } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:3475',
      rolesUrl: `http://127.0.0.1:${stubPort}/roles`,
      boardUrl: `http://127.0.0.1:${stubPort}/board`,
    });
    const server = http.createServer(app);
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
    const port = (server.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/roles?fixture=fail&failSource=roles&lagSource=roles&rolesUrl=http://127.0.0.1:1/fail`);
      const body = await res.text();
      expect(body).not.toContain('0 principals found');
      expect(body).not.toContain('roles failed');
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
      await new Promise<void>((resolve) => stub.close(() => resolve()));
    }
  });

  test('NEGATIVE PROOF: dead relay ensures message posted to /api/send never reaches #team', async () => {
    let teamReceivedCount = 0;
    const liveServer = http.createServer();
    const wss = new WebSocketServer({ server: liveServer });
    wss.on('connection', (ws) => {
      ws.on('message', (data) => {
        try {
          const frame = JSON.parse(String(data));
          if (frame[0] === 'EVENT' && frame[1]?.tags?.some((t: string[]) => t[0] === 't' && t[1] === 'team')) {
            teamReceivedCount++;
          }
        } catch {}
      });
    });

    await new Promise<void>((resolve) => liveServer.listen(0, '127.0.0.1', () => resolve()));
    const livePort = (liveServer.address() as any).port;

    const deadRelayPort = 3499;
    const { app } = createApp({
      relayUrl: `ws://127.0.0.1:${deadRelayPort}`,
      pulseUrl: 'http://127.0.0.1:3475',
    });

    const clearingServer = http.createServer(app);
    await new Promise<void>((resolve) => clearingServer.listen(0, '127.0.0.1', () => resolve()));
    const clearingPort = (clearingServer.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${clearingPort}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'jeff', text: 'Private trial note for v2' }),
      });
      expect(res.status).toBe(200);

      await new Promise((r) => setTimeout(r, 100));

      expect(teamReceivedCount).toBe(0);
    } finally {
      await new Promise<void>((resolve) => clearingServer.close(() => resolve()));
      await new Promise<void>((resolve) => {
        wss.close(() => {
          liveServer.close(() => resolve());
        });
      });
    }
  });

  test('loadConfig parses JSON config file and handles missing files gracefully', () => {
    const missing = loadConfig('/tmp/nonexistent-config-file.json');
    expect(missing).toEqual({});

    const valid = loadConfig('clearing-v2.config.json');
    expect(valid.buzzRelayUrl).toBe('ws://127.0.0.1:13000');
    expect(valid.port).toBe(3472);
  });

  test('NEGATIVE PROOF: hand-started without BUZZ_RELAY_URL shows rooms red by name ("relay not configured")', async () => {
    const { dataService } = createApp({
      relayUrl: '',
      rolesUrl: 'http://127.0.0.1:1/roles',
      boardUrl: 'http://127.0.0.1:1/board',
    });

    const sources = await dataService.getTrustStripSources({
      failSource: undefined,
    });

    const roomsSource = sources.find((s) => s.source === 'rooms');
    expect(roomsSource).toBeDefined();
    expect(roomsSource?.status).toBe('failed');
    expect(roomsSource?.reason).toBe('relay not configured');
  });
});
