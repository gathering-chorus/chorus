// @test-type: unit — verifies clearing-v2 room switching and message scoping.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import http from 'http';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from '../src/server';
import { renderRoomsPage } from '../src/views';
import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

describe('Rooms AC: Switching Rooms & Direct Conversations', () => {
  let server: http.Server;
  let port: number;

  const testPrivKeyBytes = new Uint8Array(32).fill(42);
  const testPubKeyHex = bytesToHex(schnorr.getPublicKey(testPrivKeyBytes));
  const knownKeys = new Map([[testPubKeyHex, 'abby-normal']]);

  function createSignedEvent(privKey: Uint8Array, content: string, room = 'team'): NostrEvent {
    const pubkey = bytesToHex(schnorr.getPublicKey(privKey));
    const createdAt = Math.floor(Date.now() / 1000);
    const tags = [['t', room]];
    const serialized = serializeEvent(pubkey, createdAt, 1, tags, content);
    const id = buildEventId(serialized);
    const sig = bytesToHex(schnorr.sign(hexToBytes(id), privKey));
    return { id, pubkey, created_at: createdAt, kind: 1, tags, content, sig };
  }

  beforeAll(async () => {
    const created = createApp({
      relayUrl: 'ws://127.0.0.1:3499', // dead port in all tests
      pulseUrl: 'http://127.0.0.1:9',
      knownKeys,
    });
    server = http.createServer(created.app);
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()));
    port = (server.address() as any).port;
  });


  afterAll(async () => {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  test('1. Jeff switches between team rooms and sees active room highlight and header', async () => {
    const rendered = renderRoomsPage('gathering', {
      room: 'gathering',
      channels: ['team', 'gathering', 'chorus'],
      directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
      messages: [],
    }, []);
    expect(rendered).toContain('id="room-picker"');
    expect(rendered).toContain('<option value="gathering" selected># gathering</option>');

    const res = await fetch(`http://127.0.0.1:${port}/rooms/gathering`);
    expect(res.status).toBe(200);
    const html = await res.text();

    // Room picker has rooms list
    expect(html).toContain('value="team"');
    expect(html).toContain('value="gathering"');

    // Gathering is highlighted active (selected)
    expect(html).toContain('<option value="gathering" selected># gathering</option>');

    // Negative proof: team is NOT active
    expect(html).not.toContain('<option value="team" selected>');

    // Negative proof: Dead relay must show 'relay disconnected', never 'live relay' (#3734)
    expect(html).toContain('relay disconnected');
    expect(html).not.toContain('live relay');
  });

  test('2. Jeff switches to direct conversation and sees active direct role highlighted', async () => {
    const rendered = renderRoomsPage('wren', {
      room: 'wren',
      channels: ['team', 'gathering', 'chorus'],
      directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
      messages: [],
    }, []);
    expect(rendered).toContain('id="room-picker"');
    expect(rendered).toContain('<option value="wren" selected>@ Wren</option>');

    const res = await fetch(`http://127.0.0.1:${port}/rooms/wren`);
    expect(res.status).toBe(200);
    const html = await res.text();

    // Direct roles are present in room picker
    expect(html).toContain('value="wren"');
    expect(html).toContain('value="silas"');
    expect(html).toContain('value="kade"');
    expect(html).toContain('value="abby-normal"');

    // Wren is selected in room picker
    expect(html).toContain('<option value="wren" selected>@ Wren</option>');
  });

  test('3. NEGATIVE PROOF: messages are scoped by room and do not leak across rooms', async () => {
    const rendered = renderRoomsPage('team', {
      room: 'team',
      channels: ['team', 'gathering', 'chorus'],
      directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
      messages: [],
    }, []);
    expect(rendered).toBeDefined();

    // Post to gathering room
    const gatheringEvent = createSignedEvent(testPrivKeyBytes, 'Checking gathering status', 'gathering');
    const sendRes = await fetch(`http://127.0.0.1:${port}/api/send`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ event: gatheringEvent }),
    });
    expect(sendRes.status).toBe(200);


    // Gathering room displays the message
    const gatheringRes = await fetch(`http://127.0.0.1:${port}/rooms/gathering`);
    const gatheringHtml = await gatheringRes.text();
    expect(gatheringHtml).toContain('Checking gathering status');

    // NEGATIVE PROOF: Team room must NOT display the gathering message
    const teamRes = await fetch(`http://127.0.0.1:${port}/rooms/team`);
    const teamHtml = await teamRes.text();
    expect(teamHtml).not.toContain('Checking gathering status');
  });

  test('4. NEGATIVE PROOF: an empty room shows empty with zero fake seed messages', async () => {
    const freshApp = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
      rolesUrl: 'http://127.0.0.1:1/roles',
      boardUrl: 'http://127.0.0.1:1/board',
    });
    const s = http.createServer(freshApp.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      const res = await fetch(`http://127.0.0.1:${p}/rooms/team`);
      const html = await res.text();

      // Must NOT contain the fake seed message
      expect(html).not.toContain('Morning team');
      expect(html).not.toContain('08:52');
      expect(html).not.toContain('7 working replies');

      // An empty room shows empty
      const data = await freshApp.dataService.getRoomsData('team');
      expect(data.messages).toEqual([]);
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test('5. NEGATIVE PROOF: card references parse dynamically from #N and are not hardcoded to #4530', () => {
    const rendered = renderRoomsPage('team', {
      room: 'team',
      channels: ['team'],
      directRoles: ['Wren'],
      messages: [
        {
          id: 'msg-dynamic-card',
          from: 'silas',
          text: 'Investigating issue in card #9988 now',
          time: '12:00',
          tick: false,
        },
      ],
    }, []);

    // Must linkify #9988 dynamically
    expect(rendered).toContain('openCardDrawer(\'9988\')');
    expect(rendered).toContain('#9988</a>');

    // Drawer must not hardcode 4530
    expect(rendered).not.toContain('openCardDrawer(\'4530\')');
  });

  test('6. NEGATIVE PROOF: message timestamps format in Boston time (America/New_York), not UTC', async () => {
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      pulseUrl: 'http://127.0.0.1:9',
    });
    // Ingest a message with 17:21 UTC (which is 13:21 in Boston during EDT / UTC-4)
    appInstance.router.ingest({
      id: 'msg-utc-check',
      from: 'jeff',
      text: 'Testing timezone formatting',
      ts: '2026-10-15T17:21:00.000Z',
      type: 'jeff-input',
      visible: true,
      room: 'team',
    });

    const data = await appInstance.dataService.getRoomsData('team');
    const msg = data.messages.find(m => m.id === 'msg-utc-check');
    expect(msg).toBeDefined();

    // Must be 13:21 (Boston EDT), NOT 17:21 (UTC)
    expect(msg?.time).toBe('13:21');
  });

  test('7. NEGATIVE PROOF: relay client subscribes with limit and reply tag to backfill history on connect', () => {
    const mockWs = new MockWebSocket();
    const client = new BuzzRelayClient({
      relayUrl: 'ws://127.0.0.1:3499',
      topic: 'team',
      wsFactory: () => mockWs,
    });
    client.connect();
    mockWs.triggerOpen();

    expect(mockWs.sentMessages.length).toBe(1);
    const frame = JSON.parse(mockWs.sentMessages[0]);
    expect(frame[0]).toBe('REQ');
    const filter = frame[3]; // #4652 — [2] is the channel's kind 9, [3] the kind 1 history

    // Must request limit for historical backfill
    expect(filter.limit).toBe(200);
    // Must include reply tag for role responses
    expect(filter['#t']).toContain('reply');
    expect(filter['#t']).toContain('team');
  });

  test('8. NEGATIVE PROOF: unconfirmed post to dead relay has NO delivery tick; relay event has tick', async () => {
    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499', // dead relay
      pulseUrl: 'http://127.0.0.1:9',
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      // 1. Unconfirmed local post without delivery tick
      appInstance.router.ingest({
        id: 'msg-local-unconfirmed',
        from: 'jeff',
        text: 'Unconfirmed local post',
        ts: '2026-10-15T17:20:00.000Z',
        type: 'jeff-input',
        visible: true,
        room: 'team',
        tick: false,
      });

      const res1 = await fetch(`http://127.0.0.1:${p}/rooms/team`);

      const html1 = await res1.text();
      // Unconfirmed post must NOT have delivery tick
      expect(html1).toContain('Unconfirmed local post');
      const unconfirmedSnippet = html1.slice(html1.indexOf('Unconfirmed local post') - 200, html1.indexOf('Unconfirmed local post') + 200);
      expect(unconfirmedSnippet).not.toContain('✓ relay');

      // 2. Incoming verified note from relay gets delivery tick
      appInstance.router.ingest({
        id: 'msg-relay-confirmed',
        from: 'wren',
        text: 'Confirmed relay response',
        ts: '2026-10-15T17:22:00.000Z',
        type: 'role-response',
        visible: true,
        room: 'team',
        tick: true,
      });

      const res2 = await fetch(`http://127.0.0.1:${p}/rooms/team`);
      const html2 = await res2.text();
      expect(html2).toContain('Confirmed relay response');
      const confirmedSnippet = html2.slice(html2.indexOf('Confirmed relay response') - 200, html2.indexOf('Confirmed relay response') + 200);
      expect(confirmedSnippet).toContain('✓ relay');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
    }
  });

  test('9. NEGATIVE PROOF: card side drawer fetches real card from cards door, not static inspection placeholder', async () => {
    // Spin up stub cards door server returning card #7777
    const stubCardsDoor = http.createServer((req, res) => {
      if (req.url?.includes('7777')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          apiVersion: 'v1',
          kind: 'Card',
          id: 'chorus:7777',
          data: {
            label: 'Fix wake line dropped keystroke',
            status: 'WIP',
          },
          links: {
            assignee: 'chorus:role-kade',
            ownedBy: 'chorus:principal-wren',
          },
        }));
        return;
      }
      res.writeHead(404).end();
    });
    await new Promise<void>((r) => stubCardsDoor.listen(0, '127.0.0.1', () => r()));
    const cardsPort = (stubCardsDoor.address() as any).port;

    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      cardsUrl: `http://127.0.0.1:${cardsPort}/v1/cards/cards`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      // 1. /api/cards/7777 returns the card from cards door
      const res = await fetch(`http://127.0.0.1:${p}/api/cards/7777`);
      expect(res.status).toBe(200);
      const json = await res.json() as any;
      expect(json.ok).toBe(true);
      expect(json.card.title).toBe('Fix wake line dropped keystroke');
      expect(json.card.owner).toBe('Kade');

      // 2. NEGATIVE PROOF: client script in /rooms/:room fetches /api/cards/:id and updates drawer
      const roomsRes = await fetch(`http://127.0.0.1:${p}/rooms/team`);
      const html = await roomsRes.text();
      // Must contain fetch to /api/cards/
      expect(html).toContain('/api/cards/');
      // Must NOT contain static ungrounded placeholder
      expect(html).not.toContain('Side drawer inspection in place');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
      await new Promise<void>((r) => stubCardsDoor.close(() => r()));
    }
  });

  test('10. NEGATIVE PROOF: card resolves status and owner strictly from cards door (assignee over ownedBy, no WIP literal fallback)', async () => {
    const stubCardsDoor = http.createServer((req, res) => {
      if (req.url?.includes('4530')) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          apiVersion: 'v1',
          kind: 'Card',
          id: 'chorus:4530',
          data: {
            label: 'Clearing v2 on Buzz + Pulse (:3472), side by side with the current Clearing',
            priority: 'P2',
            status: 'Next',
          },
          links: {
            assignee: 'chorus:role-abby-normal',
            ownedBy: 'chorus:principal-wren',
          },
        }));
        return;
      }
      if (req.url?.includes('9999')) {
        // Card with no status field
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          apiVersion: 'v1',
          kind: 'Card',
          id: 'chorus:9999',
          data: {
            label: 'Untriaged card without status',
          },
          links: {
            assignee: 'chorus:role-kade',
          },
        }));
        return;
      }
      res.writeHead(404).end();
    });
    await new Promise<void>((r) => stubCardsDoor.listen(0, '127.0.0.1', () => r()));
    const doorPort = (stubCardsDoor.address() as any).port;

    const appInstance = createApp({
      relayUrl: 'ws://127.0.0.1:3499',
      cardsUrl: `http://127.0.0.1:${doorPort}/v1/cards/cards`,
    });
    const s = http.createServer(appInstance.app);
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    const p = (s.address() as any).port;

    try {
      // 1. #4530 reads assignee (Abby Normal), NOT ownedBy (Wren)
      const res4530 = await fetch(`http://127.0.0.1:${p}/api/cards/4530`);
      expect(res4530.status).toBe(200);
      const json4530 = await res4530.json() as any;
      expect(json4530.ok).toBe(true);
      expect(json4530.card.owner).toBe('Abby Normal');
      expect(json4530.card.owner).not.toBe('Wren');
      expect(json4530.card.status).toBe('Next');

      // 2. Card with missing status does NOT fall back to literal 'WIP'
      const res9999 = await fetch(`http://127.0.0.1:${p}/api/cards/9999`);
      expect(res9999.status).toBe(200);
      const json9999 = await res9999.json() as any;
      expect(json9999.ok).toBe(true);
      expect(json9999.card.status).toBeUndefined();
      expect(json9999.card.status).not.toBe('WIP');
    } finally {
      await new Promise<void>((r) => s.close(() => r()));
      await new Promise<void>((r) => stubCardsDoor.close(() => r()));
    }
  });
});
