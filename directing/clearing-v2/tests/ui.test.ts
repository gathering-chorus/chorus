// @test-type: unit — verifies clearing-v2 frontend client formatting logic, no DOM, no live services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { MessageRouter } from '../src/router';
const { formatMessageTime, formatAuthor } = require('../public/app.js');

describe('Clearing v2 Frontend Helpers', () => {
  test('formatAuthor formats abby-normal with proper capitalization', () => {
    const router = new MessageRouter();
    const author = formatAuthor('abby-normal');
    expect(author).toBe('Abby Normal');
    expect(router.getRecent(1)).toEqual([]);
  });

  test('formatAuthor formats borg-queen with proper capitalization and styling support', () => {
    expect(formatAuthor('borg-queen')).toBe('Borg Queen');
    expect(formatAuthor('borg-queen')).not.toBe('borg-queen');
  });

  test('formatAuthor preserves other role names', () => {
    const router = new MessageRouter();
    const author = formatAuthor('wren');
    expect(author).toBe('wren');
    expect(router.getRecent(1)).toEqual([]);
  });

  test('formatMessageTime formats timestamp to Boston time (HH:mm) (#3734 negative proof)', () => {
    const router = new MessageRouter();
    // 19:00 UTC = 15:00 Boston time (EDT, UTC-4)
    const formatted = formatMessageTime('2026-10-15T19:00:00.000Z');
    expect(formatted).toBe('15:00');
    // NEGATIVE PROOF: must not format in UTC or un-localized time
    expect(formatted).not.toBe('19:00');
    expect(router.getRecent(1)).toEqual([]);
  });

  test('appendMessage renders delivery confirmation tick when msg.tick is true (#3734 negative proof)', () => {
    const router = new MessageRouter();
    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    (global as any).document = {
      getElementById: (id: string) => (id === 'messages-stream' ? container : null),
      createElement: (tag: string) => {
        const el: any = {
          tag,
          className: '',
          id: '',
          textContent: '',
          children: [],
          appendChild: (c: any) => el.children.push(c),
        };
        return el;
      },
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };

    jest.isolateModules(() => {
      const { appendMessage } = require('../public/app.js');

      // 1. Message with tick: true renders span.tick.delivery-tick with '✓ relay'
      appendMessage({
        id: 'msg-with-tick',
        from: 'jeff',
        text: 'Morning team',
        ts: '2026-10-15T19:00:00.000Z',
        visible: true,
        tick: true,
      });

      const msgEl = container.children.find((c: any) => c.id === 'msg-msg-with-tick');
      expect(msgEl).toBeDefined();
      const headerEl = msgEl.children.find((c: any) => c.className === 'message-header');
      expect(headerEl).toBeDefined();
      const tickEl = headerEl.children.find((c: any) => c.className === 'tick delivery-tick');
      expect(tickEl).toBeDefined();
      expect(tickEl.textContent).toBe('✓ relay');

      // 2. NEGATIVE PROOF: Message with tick: false or omitted does NOT render delivery tick
      appendMessage({
        id: 'msg-no-tick',
        from: 'wren',
        text: 'Acknowledged',
        ts: '2026-10-15T19:01:00.000Z',
        visible: true,
        tick: false,
      });

      const msgNoTickEl = container.children.find((c: any) => c.id === 'msg-msg-no-tick');
      expect(msgNoTickEl).toBeDefined();
      const headerNoTickEl = msgNoTickEl.children.find((c: any) => c.className === 'message-header');
      expect(headerNoTickEl).toBeDefined();
      const tickMissing = headerNoTickEl.children.find((c: any) => c.className === 'tick delivery-tick');
      expect(tickMissing).toBeUndefined();
    });

    expect(router.getRecent(1)).toEqual([]);
  });

  describe('Browser Key Signing (#3827, a key per WebID)', () => {
    test('loadOrCreateRoomKey returns a Schnorr keypair for a WebID and persists in storage', () => {
      const router = new MessageRouter();
      const mockStorage: Record<string, string> = {};
      const fakeLocalStorage = {
        getItem: (k: string) => mockStorage[k] || null,
        setItem: (k: string, v: string) => { mockStorage[k] = v; },
      };

      const { loadOrCreateRoomKey } = require('../public/app.js');
      const webid = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';

      const key1 = loadOrCreateRoomKey(webid, fakeLocalStorage);
      expect(key1).toBeDefined();
      expect(key1.pubkey).toMatch(/^[0-9a-f]{64}$/);
      expect(key1.fresh).toBe(true);
      expect(mockStorage['chorus.room.key.v1']).toBeDefined();

      // Loading a second time returns the same key with fresh = false
      const key2 = loadOrCreateRoomKey(webid, fakeLocalStorage);
      expect(key2.pubkey).toBe(key1.pubkey);
      expect(key2.fresh).toBe(false);
      expect(router.getRecent(1)).toEqual([]);
    });

    test('signEvent produces canonical Nostr event verifiable by verifyNostrEvent', () => {
      const router = new MessageRouter();
      const { loadOrCreateRoomKey, signEvent } = require('../public/app.js');
      const { verifyNostrEvent } = require('../src/identity');

      const mockStorage: Record<string, string> = {};
      const fakeLocalStorage = {
        getItem: (k: string) => mockStorage[k] || null,
        setItem: (k: string, v: string) => { mockStorage[k] = v; },
      };

      const webid = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
      const { sk, pubkey } = loadOrCreateRoomKey(webid, fakeLocalStorage);

      const event = signEvent(sk, 'Hello from Jeff phone', { room: 'team' });
      expect(event.pubkey).toBe(pubkey);
      expect(event.content).toBe('Hello from Jeff phone');
      expect(event.kind).toBe(1);
      expect(event.tags).toEqual([['room', 'team']]);
      expect(event.id).toMatch(/^[0-9a-f]{64}$/);
      expect(event.sig).toMatch(/^[0-9a-f]{128}$/);

      // Verifies using official Nostr Schnorr verification
      expect(verifyNostrEvent(event)).toBe(true);

      // NEGATIVE PROOF: Altered content invalidates event
      const tampered = { ...event, content: 'Tampered content' };
      expect(verifyNostrEvent(tampered)).toBe(false);
      expect(router.getRecent(1)).toEqual([]);
    });

    test('NEGATIVE PROOF: loadOrCreateRoomKey refuses when webid is missing', () => {
      const router = new MessageRouter();
      const { loadOrCreateRoomKey } = require('../public/app.js');
      expect(() => loadOrCreateRoomKey('', {})).toThrow(/no WebID/i);
      expect(() => loadOrCreateRoomKey(null, {})).toThrow(/no WebID/i);
      expect(router.getRecent(1)).toEqual([]);
    });
  });

  describe('Failed Send Handling (show failed send by name, restore text)', () => {
    test('surfaces failed send error by name and restores composer input text', async () => {
      const router = new MessageRouter();
      const { handleSendMessage } = require('../public/app.js');

      const mockInput = { value: 'Important unsent message' };
      const mockStatus = { textContent: '', style: { display: 'none' } };
      const mockBtn = { textContent: 'Send', className: '', classList: { add: jest.fn(), remove: jest.fn() } };

      const fakeFetch = jest.fn().mockResolvedValue({
        ok: false,
        status: 401,
        json: async () => ({ ok: false, error: 'unsigned or unregistered post refused: valid signed event from registered key required (#3743)' }),
        text: async () => 'unsigned or unregistered post refused',
      });

      await handleSendMessage('Important unsent message', {
        inputEl: mockInput,
        statusEl: mockStatus,
        btnEl: mockBtn,
        fetchFn: fakeFetch,
      });

      // 1. Shows failed error by name
      expect(mockStatus.textContent).toMatch(/unsigned or unregistered post refused/i);
      expect(mockStatus.style.display).not.toBe('none');

      // 2. Restores input value so Jeff does not lose typed text
      expect(mockInput.value).toBe('Important unsent message');
      expect(router.getRecent(1)).toEqual([]);
    });

    test('NEGATIVE PROOF: successful send clears input and does not display failure error', async () => {
      const router = new MessageRouter();
      const { handleSendMessage } = require('../public/app.js');

      const mockInput = { value: 'Good morning' };
      const mockStatus = { textContent: '', style: { display: 'none' } };
      const mockBtn = { textContent: 'Send', className: '', classList: { add: jest.fn(), remove: jest.fn() } };

      const fakeFetch = jest.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({ ok: true, id: 'event-123', from: 'jeff' }),
      });

      await handleSendMessage('Good morning', {
        inputEl: mockInput,
        statusEl: mockStatus,
        btnEl: mockBtn,
        fetchFn: fakeFetch,
      });

      // Input is cleared
      expect(mockInput.value).toBe('');
      // No failure text
      expect(mockStatus.textContent).not.toMatch(/failed/i);
      expect(mockBtn.textContent).toBe('Sent');
      expect(router.getRecent(1)).toEqual([]);
    });
  });
});


// ---- #4530 room key (public/room-key.js), merged here so the registered UI suite exercises it
// #4530 part 1 — the browser's room key (public/room-key.js, v1 #3827 rule): one key
// per WebID, kept across loads, never shared between people, and events it signs
// verify. room-key.js is a browser module (absolute /vendor imports), so the test
// loads its source with the same noble libraries the page is served.
import * as fs from 'fs';
import * as path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { verifyNostrEvent } from '../src/identity';

type RoomKey = {
  loadOrCreate: (webid: string, s?: Storage) => { sk: Uint8Array; pubkey: string; fresh: boolean };
  signEvent: (sk: Uint8Array, kind: number, tags: string[][], content: string) => any;
};

function loadRoomKey(): RoomKey {
  const src = fs.readFileSync(path.join(__dirname, '../public/room-key.js'), 'utf8')
    .replace(/^import .*$/gm, '')
    .replace(/^export (async )?function/gm, '$1function')
    .replace(/^if \(typeof window[\s\S]*$/m, '');
  return new Function('schnorr', 'nobleSha256', `${src}\nreturn { loadOrCreate, signEvent };`)(schnorr, sha256);
}

function memStorage(): Storage {
  const m = new Map<string, string>();
  return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v) } as Storage;
}

const JEFF = 'https://id.example.test/jeff/profile/card#me';
const MARK = 'https://id.example.test/mark/profile/card#me';

test('the same WebID gets the same key across loads', () => {
  const rk = loadRoomKey();
  const s = memStorage();
  const a = rk.loadOrCreate(JEFF, s);
  const b = rk.loadOrCreate(JEFF, s);
  expect(a.fresh).toBe(true);
  expect(b.fresh).toBe(false);
  expect(b.pubkey).toBe(a.pubkey);
});

test('two people sharing a browser never share a key', () => {
  const rk = loadRoomKey();
  const s = memStorage();
  expect(rk.loadOrCreate(MARK, s).pubkey).not.toBe(rk.loadOrCreate(JEFF, s).pubkey);
});

test('no WebID refuses by name instead of minting a key for nobody', () => {
  expect(() => loadRoomKey().loadOrCreate('', memStorage())).toThrow(/not signed in/);
});

test('a signed event verifies, and a tampered one does not (negative proof)', () => {
  const rk = loadRoomKey();
  const { sk } = rk.loadOrCreate(JEFF, memStorage());
  const ev = rk.signEvent(sk, 1, [['t', 'team']], 'morning');
  expect(verifyNostrEvent(ev)).toBe(true);
  expect(verifyNostrEvent({ ...ev, content: 'evening' })).toBe(false);
});

// #4574 — Abby's catch: live and history messages rendered #N as plain text, so no card link
// in Rooms opened. Every #N is now a link built from DOM nodes; markup in a message stays text.
describe('room messages link #N to the card (#4574)', () => {
  test('a #N becomes a card link; the rest stays text, markup included (negative proof)', () => {
    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    const mk = (tag: string): any => { const el: any = { tag, className: '', id: '', textContent: '', href: '', children: [], appendChild: (c: any) => el.children.push(c) }; return el; };
    (global as any).document = {
      getElementById: (id: string) => (id === 'messages-stream' ? container : null),
      createElement: mk,
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };
    jest.isolateModules(() => {
      const { appendMessage } = require('../public/app.js');
      appendMessage({ id: 'm1', from: 'jeff', text: 'see #4574 and <b>#12</b>', ts: '2026-10-17T08:00:00.000Z', visible: true });
      const text = container.children[0].children.find((c: any) => c.className === 'message-text');
      const links = text.children.filter((c: any) => c.tag === 'a');
      expect(links.map((a: any) => a.textContent)).toEqual(['#4574', '#12']);
      expect(links[0].className).toContain('card-ref');
      const plain = text.children.filter((c: any) => c.tag === '#text').map((c: any) => c.textContent).join('|');
      expect(plain).toBe('see | and <b>|</b>');
    });
  });
});

describe('#4575: DOM id match, duplicate suppression, and safe markdown bold/code', () => {
  test('renderRoomsPage uses id="msg-<id>" matching appendMessage, and same event id twice renders one row (negative proof)', () => {
    const { renderRoomsPage } = require('../src/views');
    const html = renderRoomsPage('team', {
      room: 'team',
      channels: ['team'],
      directRoles: ['wren'],
      relayConnected: true,
      messages: [{ id: 'ev-dup-42', from: 'kade', text: '**Bold** and `code` on #4575 <img src=x onerror=alert(1)>', time: '06:05', tick: true }],
    }, []);
    expect(html).toContain('id="msg-ev-dup-42"');
    expect(html).toContain('<strong>Bold</strong>');
    expect(html).toContain('<code>code</code>');
    expect(html).not.toContain('**Bold**');
    expect(html).not.toContain('<img src=x');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');

    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    const mk = (tag: string): any => ({ tag, className: '', id: '', textContent: '', href: '', dataset: {}, children: [], appendChild(c: any) { this.children.push(c); } });
    (global as any).document = {
      getElementById: (id: string) => {
        if (id === 'messages-stream') return container;
        return container.children.find((c: any) => c.id === id) || null;
      },
      createElement: mk,
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };

    jest.isolateModules(() => {
      const { appendMessage } = require('../public/app.js');
      appendMessage({ id: 'ev-dup-42', from: 'kade', text: '**Bold** and `code`', ts: '2026-10-17T10:05:00.000Z', visible: true });
      appendMessage({ id: 'ev-dup-42', from: 'kade', text: '**Bold** and `code`', ts: '2026-10-17T10:05:00.000Z', visible: true });
      expect(container.children).toHaveLength(1);
      const textEl = container.children[0].children.find((c: any) => c.className === 'message-text');
      const strongs = textEl.children.filter((c: any) => c.tag === 'strong');
      const codes = textEl.children.filter((c: any) => c.tag === 'code');
      expect(strongs.map((s: any) => s.textContent)).toEqual(['Bold']);
      expect(codes.map((c: any) => c.textContent)).toEqual(['code']);
    });
  });
});

// #4575 — Abby's catch: a live message first seen as v1's bridge mirror kept the name
// "bridge" on screen when the role's own copy arrived; only the data attribute changed.
describe('live bridge copy takes the real author name (#4575)', () => {
  test('the visible name changes to the role; a bridge-only message keeps bridge (negative proof)', () => {
    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    const find = (el: any, cls: string): any => {
      for (const c of el.children || []) {
        if (typeof c.className === 'string' && c.className.split(' ').includes(cls)) return c;
        const hit = find(c, cls);
        if (hit) return hit;
      }
      return null;
    };
    const mk = (tag: string): any => {
      const el: any = { tag, className: '', id: '', textContent: '', dataset: {}, children: [], appendChild: (c: any) => el.children.push(c) };
      el.querySelector = (sel: string) => find(el, sel.replace(/^\./, ''));
      return el;
    };
    (global as any).document = {
      getElementById: (id: string) => (id === 'messages-stream' ? container : container.children.find((c: any) => c.id === id) || null),
      createElement: mk,
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };
    jest.isolateModules(() => {
      const { appendMessage } = require('../public/app.js');
      appendMessage({ id: 'b1', from: 'bridge', text: 'build is green', ts: '2026-10-17T10:40:00.000Z', visible: true });
      appendMessage({ id: 'k1', from: 'kade', text: 'build is green', ts: '2026-10-17T10:40:01.000Z', visible: true });
      appendMessage({ id: 'b2', from: 'bridge', text: 'jeff via v1', ts: '2026-10-17T10:41:00.000Z', visible: true });
      expect(container.children).toHaveLength(2);
      expect(find(container.children[0], 'message-author').textContent.toLowerCase()).toContain('kade');
      expect(find(container.children[1], 'message-author').textContent.toLowerCase()).toContain('bridge');
    });
  });
});
