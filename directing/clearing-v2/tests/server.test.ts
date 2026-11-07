// @test-type: unit — verifies clearing-v2 server route mounting and tile-service state, no live network services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import path from 'path';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import fs from 'fs';
import os from 'os';
import { createApp, loadConfig } from '../src/server';
import { TileService } from '../src/tile-service';
import { serializeEvent, buildEventId, verifyNostrEvent, resolveRoleByPubkey } from '../src/identity';
import { mintShareSessionCookie } from '../src/share-session';
import { NostrEvent } from '../src/types';

describe('Clearing v2: Server App & Tile Service', () => {
  const testPrivKeyBytes = new Uint8Array(32).fill(42);
  const testPubKeyHex = bytesToHex(schnorr.getPublicKey(testPrivKeyBytes));

  const unregisteredPrivKeyBytes = new Uint8Array(32).fill(99);
  const unregisteredPubKeyHex = bytesToHex(schnorr.getPublicKey(unregisteredPrivKeyBytes));

  function createSignedEvent(privKey: Uint8Array, content: string, room = 'team'): NostrEvent {
    const pubkey = bytesToHex(schnorr.getPublicKey(privKey));
    const createdAt = Math.floor(Date.now() / 1000);
    const tags = [['t', room]];
    const serialized = serializeEvent(pubkey, createdAt, 1, tags, content);
    const id = buildEventId(serialized);
    const sig = bytesToHex(schnorr.sign(hexToBytes(id), privKey));
    return { id, pubkey, created_at: createdAt, kind: 1, tags, content, sig };
  }

  test('createApp initializes app, router, relay and tileService', () => {
    const { app, router, tileService, relay } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
    });
    expect(app).toBeDefined();
    expect(router).toBeDefined();
    expect(tileService).toBeDefined();
    expect(relay).toBeDefined();
  });

  test('TileService initializes known team roles with idle state', () => {
    const tileService = new TileService('http://127.0.0.1:9');
    const tiles = tileService.getTiles();
    expect(tiles.length).toBe(6);
    const roles = tiles.map(t => t.role);
    expect(roles).toContain('abby-normal');
    expect(roles).toContain('wren');
    expect(roles).toContain('silas');
    expect(roles).toContain('kade');
    expect(roles).toContain('jeff');
    expect(roles).toContain('borg-queen');
    const abbyTile = tiles.find(t => t.role === 'abby-normal');
    expect(abbyTile?.name).toBe('Abby Normal');
    expect(abbyTile?.state).toBe('idle');
    const bqTile = tiles.find(t => t.role === 'borg-queen');
    expect(bqTile?.name).toBe('Borg Queen');
    expect(bqTile?.state).toBe('idle');
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
    expect(tiles.length).toBe(6);
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
      pulseUrl: 'http://127.0.0.1:9',
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
        } catch { /* a non-JSON relay frame is not a #team message; count only parsed ones */ }
      });
    });

    await new Promise<void>((resolve) => liveServer.listen(0, '127.0.0.1', () => resolve()));
    const livePort = (liveServer.address() as any).port;

    const deadRelayPort = 3499;
    const { app } = createApp({
      relayUrl: `ws://127.0.0.1:${deadRelayPort}`,
      pulseUrl: 'http://127.0.0.1:9',
      knownKeys: new Map([[testPubKeyHex, 'abby-normal']]),
    });

    const clearingServer = http.createServer(app);
    await new Promise<void>((resolve) => clearingServer.listen(0, '127.0.0.1', () => resolve()));
    const clearingPort = (clearingServer.address() as any).port;

    try {
      const validEvent = createSignedEvent(testPrivKeyBytes, 'Private trial note for v2');
      const res = await fetch(`http://127.0.0.1:${clearingPort}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ event: validEvent }),
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

    const sources = await dataService.getTrustStripSources();

    const roomsSource = sources.find((s) => s.source === 'rooms');
    expect(roomsSource).toBeDefined();
    expect(roomsSource?.status).toBe('failed');
    expect(roomsSource?.reason).toBe('relay not configured');
  });

  test('NEGATIVE PROOF: /api/send refuses unsigned posts with 401', async () => {
    const { app } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'jeff', text: 'Unsigned spoof attempt' }),
      });
      expect(res.status).toBe(401);
      const json = await res.json() as any;
      expect(json.ok).toBe(false);
      expect(json.error).toMatch(/unsigned|signed/i);
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('NEGATIVE PROOF: /api/send refuses events signed by unregistered keys with 401', async () => {
    const { app } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
      knownKeys: new Map([[testPubKeyHex, 'abby-normal']]),
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const unregisteredEvent = createSignedEvent(unregisteredPrivKeyBytes, 'Post from rogue agent');
      const res = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ event: unregisteredEvent }),
      });
      expect(res.status).toBe(401);
      const json = await res.json() as any;
      expect(json.ok).toBe(false);
      expect(json.error).toBe('not-signed-in'); // #4571: an unregistered key is refused as not signed in, naming the door
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('accepts event signed by registered key, taking sender strictly from the key', async () => {
    const { app, router } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
      knownKeys: new Map([[testPubKeyHex, 'abby-normal']]),
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const validEvent = createSignedEvent(testPrivKeyBytes, 'Signed message from Abby Normal');
      const res = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ from: 'jeff', event: validEvent }),
      });
      expect(res.status).toBe(200);
      const json = await res.json() as any;
      expect(json.ok).toBe(true);
      expect(json.from).toBe('abby-normal');

      const recent = router.getRecent(1);
      expect(recent.length).toBe(1);
      expect(recent[0].from).toBe('abby-normal');
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  });

  test('POST /api/room/bind requires verified session cookie, refusing unauthenticated with 401 (#3827)', async () => {
    const jeffInputs: Array<{ to: string; content: string; caller: string | undefined }> = [];
    const pulseStub = http.createServer((req, res) => {
      let body = '';
      req.on('data', (c) => { body += String(c); });
      req.on('end', () => {
        if (req.method === 'POST' && req.url === '/api/jeff-input') {
          const parsed = JSON.parse(body);
          jeffInputs.push({ to: parsed.to, content: parsed.content, caller: req.headers['x-chorus-clearing-caller'] as string | undefined });
        }
        res.writeHead(202, { 'Content-Type': 'application/json' });
        res.end('{"ok":true}');
      });
    });
    await new Promise<void>((r) => pulseStub.listen(0, '127.0.0.1', () => r()));
    const pulsePort = (pulseStub.address() as any).port;

    const testCookieKey = Buffer.from('test-secret-cookie-key-for-session-32b');
    const { app, router } = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: `http://127.0.0.1:${pulsePort}`,
      knownKeys: new Map([[testPubKeyHex, 'abby-normal']]),
      cookieKey: testCookieKey,
    });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as any).port;

    try {
      const browserPrivKey = new Uint8Array(32).fill(77);
      const browserPubKey = bytesToHex(schnorr.getPublicKey(browserPrivKey));
      const jeffWebId = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
      const event = createSignedEvent(browserPrivKey, 'Hello team from Jeff browser');

      // 1. NEGATIVE PROOF: /api/room/bind with no session gets 401
      const unauthedBindRes = await fetch(`http://127.0.0.1:${port}/api/room/bind`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pubkey: browserPubKey, webid: jeffWebId }),
      });
      expect(unauthedBindRes.status).toBe(401);
      const unauthedJson = await unauthedBindRes.json() as any;
      expect(unauthedJson.error).toBe('not-signed-in');

      // 2. Browser joins with verified session cookie
      const validCookie = mintShareSessionCookie(jeffWebId, testCookieKey);
      const bindRes = await fetch(`http://127.0.0.1:${port}/api/room/bind`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Cookie': `chorus_share_session=${validCookie}`,
        },
        body: JSON.stringify({ pubkey: browserPubKey }),
      });
      expect(bindRes.status).toBe(200);
      const bindJson = await bindRes.json() as any;
      expect(bindJson.ok).toBe(true);
      expect(bindJson.pubkey).toBe(browserPubKey);

      // 3. After verified binding, /api/send accepts the signed event as 'jeff'
      const postBindRes = await fetch(`http://127.0.0.1:${port}/api/send`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ event }),
      });
      expect(postBindRes.status).toBe(200);
      const postBindJson = await postBindRes.json() as any;
      expect(postBindJson.ok).toBe(true);
      expect(postBindJson.from).toBe('jeff');

      const recent = router.getRecent(1);
      expect(recent.length).toBe(1);
      expect(recent[0].from).toBe('jeff');
      expect(recent[0].type).toBe('jeff-input');
      // no relay in this test, so the sent tick stays off: the tick means the relay
      // accepted it (rooms.feature:19), not that the server received it
      expect(recent[0].tick).toBe(false);
      // #4592 — wait for the five hand-offs, not a fixed 30ms: on a busy box the
      // fifth POST lands later and a timing assert reads as a product red (#4724 adds borg-queen).
      for (let i = 0; i < 100 && jeffInputs.length < 5; i++) await new Promise((r) => setTimeout(r, 20));
      expect(jeffInputs.length).toBe(5);
      expect(new Set(jeffInputs.map((i: any) => i.to))).toEqual(new Set(['wren', 'silas', 'kade', 'abby-normal', 'borg-queen']));
      expect(jeffInputs.every((i: any) => i.content === 'Hello team from Jeff browser' && i.caller === '1')).toBe(true);
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
      await new Promise<void>((r) => pulseStub.close(() => r()));
    }
  });
});

describe('Clearing v2: #4570 Service Run & WiFi Binding', () => {
  test('the config the LaunchAgent passes (--config clearing-v2.config.json) binds 0.0.0.0 on :3472 for wifi', () => {
    const cfg = loadConfig(path.join(__dirname, '..', 'clearing-v2.config.json'));
    expect(cfg.port).toBe(3472);
    expect(cfg.host).toBe('0.0.0.0');
    expect(cfg.pulseUrl).toBe('http://127.0.0.1:3475');
  });
});

describe('Clearing v2: #4571 AC3 Live Role Reply via SSE without Reload', () => {
  const wrenPriv = new Uint8Array(32).fill(55);
  const wrenPub = bytesToHex(schnorr.getPublicKey(wrenPriv));

  function signReply(privKey: Uint8Array, text: string): NostrEvent {
    const pubkey = bytesToHex(schnorr.getPublicKey(privKey));
    const createdAt = Math.floor(Date.now() / 1000);
    const tags = [['t', 'team'], ['t', 'reply']];
    const serialized = serializeEvent(pubkey, createdAt, 1, tags, text);
    const id = buildEventId(serialized);
    const sig = bytesToHex(schnorr.sign(hexToBytes(id), privKey));
    return { id, pubkey, created_at: createdAt, kind: 1, tags, content: text, sig };
  }

  test('role reply arriving from Buzz relay streams over /api/events SSE without reload, while forged event is dropped (negative proof)', async () => {
    const relayHttp = http.createServer();
    const wss = new WebSocketServer({ server: relayHttp });
    let relaySocket: any = null;
    const reqReceived = new Promise<void>((resolve) => {
      wss.on('connection', (ws) => {
        relaySocket = ws;
        ws.on('message', (raw) => {
          const frame = JSON.parse(String(raw));
          if (frame[0] === 'REQ') resolve();
        });
      });
    });

    await new Promise<void>((r) => relayHttp.listen(0, '127.0.0.1', () => r()));
    const relayPort = (relayHttp.address() as any).port;

    const { app, relay } = createApp({
      relayUrl: `ws://127.0.0.1:${relayPort}`,
      pulseUrl: 'http://127.0.0.1:9',
      knownKeys: new Map([[wrenPub, 'wren']]),
      relaySigner: null,
    });

    const clearingServer = http.createServer(app);
    await new Promise<void>((r) => clearingServer.listen(0, '127.0.0.1', () => r()));
    const clearingPort = (clearingServer.address() as any).port;

    relay.connect();
    await reqReceived;

    const sseMessages: Array<{ event: string; data: any }> = [];
    let onMessageEvent: (() => void) | null = null;
    const messageArrived = new Promise<void>((resolve) => {
      onMessageEvent = resolve;
    });

    const sseReq = http.get(`http://127.0.0.1:${clearingPort}/api/events`, (res) => {
      let buf = '';
      res.on('data', (chunk) => {
        buf += String(chunk);
        const frames = buf.split('\n\n');
        buf = frames.pop() || '';
        for (const f of frames) {
          const lines = f.split('\n');
          const evLine = lines.find((l) => l.startsWith('event: '));
          const dataLine = lines.find((l) => l.startsWith('data: '));
          if (evLine && dataLine) {
            const ev = evLine.slice(7).trim();
            const parsed = JSON.parse(dataLine.slice(6));
            sseMessages.push({ event: ev, data: parsed });
            if (ev === 'message' && onMessageEvent) onMessageEvent();
          }
        }
      });
    });

    try {
      // Wait for initial SSE init event
      await new Promise((r) => setTimeout(r, 50));

      // 1. NEGATIVE PROOF: forged/tampered relay event is dropped and never streamed to SSE
      const validReply = signReply(wrenPriv, 'Live reply from Wren without reload');
      const forgedReply: NostrEvent = { ...validReply, content: 'Tampered text after signing' };
      relaySocket.send(JSON.stringify(['EVENT', 'team-sub', forgedReply]));
      await new Promise((r) => setTimeout(r, 50));
      expect(sseMessages.filter((m) => m.event === 'message')).toHaveLength(0);

      // 2. POSITIVE PROOF: valid signed role reply from relay arrives on /api/events SSE without reload
      relaySocket.send(JSON.stringify(['EVENT', 'team-sub', validReply]));
      await messageArrived;

      const msgEvents = sseMessages.filter((m) => m.event === 'message');
      expect(msgEvents).toHaveLength(1);
      expect(msgEvents[0].data).toMatchObject({
        id: validReply.id,
        from: 'wren',
        text: 'Live reply from Wren without reload',
        type: 'role-response',
        room: 'team',
        tick: true,
      });
    } finally {
      sseReq.destroy();
      relay.close();
      await new Promise<void>((r) => clearingServer.close(() => r()));
      await new Promise<void>((r) => wss.close(() => relayHttp.close(() => r())));
    }
  });
});
