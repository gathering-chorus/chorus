// @test-type: unit — Clearing v2 history auto-backfill on actor filter (#4741)
// @card: #4741
// @owner: abby-normal
// @domain: messages

import { renderRoomsPage } from '../src/views';
import type { RoomsData } from '../src/sources/models';

let mockContainer: any;
let mockPeopleRow: any;
let mockTiles: Record<string, any>;
let mockComposerForm: any;
let mockComposerInput: any;

function createMockElement(id: string, tag: string = 'div') {
  const classListSet = new Set<string>();
  return {
    id,
    tagName: tag.toUpperCase(),
    className: '',
    classList: {
      add: (c: string) => classListSet.add(c),
      remove: (c: string) => classListSet.delete(c),
      contains: (c: string) => classListSet.has(c),
      toggle: (c: string, force?: boolean) => {
        if (force === true) { classListSet.add(c); return true; }
        if (force === false) { classListSet.delete(c); return false; }
        if (classListSet.has(c)) { classListSet.delete(c); return false; }
        classListSet.add(c);
        return true;
      },
    },
    style: { display: '', height: '' },
    dataset: {} as Record<string, string>,
    children: [] as any[],
    value: '',
    placeholder: '',
    textContent: '',
    innerText: '',
    innerHTML: '',
    scrollTop: 0,
    scrollHeight: 1000,
    clientHeight: 600,
    addEventListener: jest.fn(),
    appendChild(c: any) { this.children.push(c); },
    querySelector(sel: string) {
      if (sel === '[id^="msg-"]') {
        return this.children.find((c: any) => c.id && c.id.startsWith('msg-')) || null;
      }
      return null;
    },
    querySelectorAll(sel: string) {
      if (sel.includes('.msg') || sel.includes('.message') || sel.includes('[id^="msg-"]')) {
        return this.children || [];
      }
      return [];
    },
    insertAdjacentHTML(where: string, html: string) {
      const ids = (html.match(/id="msg-[^"]+"/g) || []).map((m) => m.replace(/id="|"/g, ''));
      for (const msgId of ids.reverse()) {
        const newEl = createMockElement(msgId, 'div');
        const fromMatch = html.match(new RegExp(`id="${msgId}"[^>]*data-from="([^"]+)"`));
        if (fromMatch) newEl.dataset.from = fromMatch[1];
        mockContainer.children.unshift(newEl);
      }
    },
    focus: jest.fn(),
  };
}

mockContainer = createMockElement('messages-stream');
mockPeopleRow = createMockElement('rooms-people-row');
mockComposerForm = createMockElement('composer-form', 'form');
mockComposerInput = createMockElement('composer-input', 'textarea');

mockTiles = {
  wren: createMockElement('tile-wren', 'a'),
  silas: createMockElement('tile-silas', 'a'),
  kade: createMockElement('tile-kade', 'a'),
  'abby-normal': createMockElement('tile-abby-normal', 'a'),
  'borg-queen': createMockElement('tile-borg-queen', 'a'),
};
mockTiles.wren.dataset.role = 'wren';
mockTiles.silas.dataset.role = 'silas';
mockTiles.kade.dataset.role = 'kade';
mockTiles['abby-normal'].dataset.role = 'abby-normal';
mockTiles['borg-queen'].dataset.role = 'borg-queen';

mockPeopleRow.children = Object.values(mockTiles);
mockPeopleRow.querySelectorAll = (sel: string) => {
  if (sel.includes('.actor-tile')) return Object.values(mockTiles);
  return [];
};

(global as any).document = {
  getElementById: (id: string) => {
    if (id === 'messages-stream') return mockContainer;
    if (id === 'rooms-people-row') return mockPeopleRow;
    if (id === 'composer-form') return mockComposerForm;
    if (id === 'composer-input') return mockComposerInput;
    if (id === 'tile-wren') return mockTiles.wren;
    if (id === 'tile-silas') return mockTiles.silas;
    if (id === 'tile-kade') return mockTiles.kade;
    if (id === 'tile-abby-normal') return mockTiles['abby-normal'];
    if (id === 'tile-borg-queen') return mockTiles['borg-queen'];
    if (id === 'room-picker') return { value: 'team' };
    return null;
  },
  querySelector: (sel: string) => {
    if (sel === '.chat') return createMockElement('chat');
    if (sel === '#composer-input') return mockComposerInput;
    if (sel === '#messages-stream') return mockContainer;
    return null;
  },
  querySelectorAll: (sel: string) => {
    if (sel.includes('.actor-tile')) return Object.values(mockTiles);
    if (sel.includes('.msg') || sel.includes('.message') || sel.includes('[id^="msg-"]')) {
      return mockContainer.children;
    }
    return [];
  },
  createElement: (tag: string) => createMockElement('elem-' + Math.random(), tag),
  createTextNode: (t: string) => ({ textContent: t }),
};

(global as any).window = {
  CLEARING_BASE: '',
  addEventListener: jest.fn(),
  location: { href: '' },
  innerWidth: 375,
  innerHeight: 667,
  scrollY: 0,
};

(global as any).EventSource = class MockEventSource {
  addEventListener() {}
  close() {}
};

const app = require('../public/app.js');

describe('#4741 Clearing v2 history auto-backfill on actor filter and bottom actor strip', () => {
  beforeEach(() => {
    if (app.setRoleFilter) app.setRoleFilter(null);
    mockContainer.children = [];
    mockContainer.scrollTop = 0;
    mockContainer.scrollHeight = 1000;
  });

  describe('AC1: Actor rail rendered below composer bar', () => {
    test('renderRoomsPage places rooms-people-row after composer-bar', () => {
      const mockData: RoomsData = {
        room: 'team',
        channels: ['team'],
        directRoles: ['abby-normal'],
        relayConnected: true,
        actors: [
          {
            id: 'abby-normal',
            name: 'Abby Normal',
            state: 'building',
            sessions: ['live'],
            cards: [],
            stream: [],
          },
        ],
        messages: [],
      };

      const html = renderRoomsPage('team', mockData, []);
      const composerIdx = html.indexOf('<footer class="composer-bar">');
      const peopleRowIdx = html.indexOf('id="rooms-people-row"');

      expect(composerIdx).toBeGreaterThan(0);
      expect(peopleRowIdx).toBeGreaterThan(0);
      expect(peopleRowIdx).toBeGreaterThan(composerIdx);
    });
  });

  describe('AC2: Auto-backfill older history when actor filter is active', () => {
    test('backfillFilteredHistory pages older messages until target visible count is met or container overflows', async () => {
      const msg1 = createMockElement('msg-1');
      msg1.dataset.from = 'abby-normal';
      msg1.textContent = 'Initial 1';

      const msg2 = createMockElement('msg-2');
      msg2.dataset.from = 'abby-normal';
      msg2.textContent = 'Initial 2';

      mockContainer.children = [msg1, msg2];
      mockContainer.scrollHeight = 1000;
      mockContainer.clientHeight = 600;

      let pageCount = 0;
      const mockFetch = jest.fn(async (_url: string) => {
        pageCount++;
        if (pageCount === 1) {
          return {
            ok: true,
            json: async () => ({
              count: 5,
              html: '<div class="msg" id="msg-p1-1" data-from="silas">silas 1</div>' +
                    '<div class="msg" id="msg-p1-2" data-from="abby-normal">abby 3</div>' +
                    '<div class="msg" id="msg-p1-3" data-from="wren">wren 1</div>' +
                    '<div class="msg" id="msg-p1-4" data-from="abby-normal">abby 4</div>' +
                    '<div class="msg" id="msg-p1-5" data-from="kade">kade 1</div>',
            }),
          };
        }
        if (pageCount === 2) {
          return {
            ok: true,
            json: async () => ({
              count: 3,
              html: '<div class="msg" id="msg-p2-1" data-from="abby-normal">abby 5</div>' +
                    '<div class="msg" id="msg-p2-2" data-from="silas">silas 2</div>' +
                    '<div class="msg" id="msg-p2-3" data-from="abby-normal">abby 6</div>',
            }),
          };
        }
        return {
          ok: true,
          json: async () => ({
            count: 0,
            html: '',
          }),
        };
      });

      await app.setRoleFilter('abby-normal', mockFetch);

      expect(mockFetch).toHaveBeenCalledTimes(3);

      const visible = mockContainer.children.filter((c: any) => c.style.display !== 'none');
      expect(visible.length).toBeGreaterThanOrEqual(5);
    });

    test('backfillFilteredHistory continues while scrollHeight <= clientHeight even if count threshold reached', async () => {
      app.setRoleFilter('abby-normal');
      mockContainer.children = [];
      for (let i = 0; i < 5; i++) {
        const m = createMockElement(`msg-fill-${i}`);
        m.dataset.from = 'abby-normal';
        mockContainer.children.push(m);
      }
      app.applyRoleFilters();

      // Viewport is 600px, but content is only 400px (non-overflowing)
      mockContainer.clientHeight = 600;
      mockContainer.scrollHeight = 400;

      let fetches = 0;
      const mockFetchOverflow = jest.fn(async () => {
        fetches++;
        if (fetches === 1) {
          mockContainer.scrollHeight = 800; // now overflows
          return {
            ok: true,
            json: async () => ({
              count: 2,
              html: '<div class="msg" id="msg-extra-1" data-from="abby-normal">extra 1</div>',
            }),
          };
        }
        return { ok: true, json: async () => ({ count: 0, html: '' }) };
      });

      // Target is 5, which is already met, but scrollHeight <= clientHeight forces backfill until overflow
      await app.backfillFilteredHistory(mockFetchOverflow, 5);
      expect(mockFetchOverflow).toHaveBeenCalled();
    });
  });

  describe('AC3: Mobile viewport scrolling remains active and triggers older message backfill when filtered', () => {
    test('scrolling up while filtered triggers watchRoomScroll backfill for older messages beyond initial 15', async () => {
      app.setRoleFilter('abby-normal');
      mockContainer.children = [];
      for (let i = 0; i < 15; i++) {
        const m = createMockElement(`msg-scrolled-${i}`);
        m.dataset.from = 'abby-normal';
        mockContainer.children.push(m);
      }
      app.applyRoleFilters();
      mockContainer.scrollHeight = 1000;
      mockContainer.clientHeight = 600;

      let scrollListener: Function | null = null;
      mockContainer.addEventListener = jest.fn((evt: string, fn: any) => {
        if (evt === 'scroll') scrollListener = fn;
      });

      const mockFetch = jest.fn(async () => ({
        ok: true,
        json: async () => ({
          count: 1,
          html: '<div class="msg" id="msg-older-1" data-from="abby-normal">older 1</div>',
        }),
      }));

      app.watchRoomScroll(mockFetch);
      expect(scrollListener).not.toBeNull();

      mockContainer.scrollTop = 0;
      await (scrollListener as any)();

      expect(mockFetch).toHaveBeenCalled();
    });
  });

  describe('AC4: Rejoining at latest message when unfiltered', () => {
    test('clearing role filter scrolls stream to newest message', () => {
      mockContainer.scrollTop = 50;
      mockContainer.scrollHeight = 800;

      app.setRoleFilter('abby-normal');
      app.setRoleFilter(null);

      expect(mockContainer.scrollTop).toBe(mockContainer.scrollHeight);
    });
  });

  describe('AC5: Negative proof (#3734)', () => {
    test('NEGATIVE PROOF: without auto-backfill, zero-match raw batch leaves visible dialogue starved', async () => {
      app.setRoleFilter('abby-normal');
      const msgInit = createMockElement('msg-init');
      msgInit.dataset.from = 'abby-normal';
      mockContainer.children = [msgInit];
      app.applyRoleFilters();

      const mockFetchZeroMatch = jest.fn(async () => ({
        ok: true,
        json: async () => ({
          count: 2,
          html: '<div class="msg" id="msg-s1" data-from="silas">silas</div><div class="msg" id="msg-w1" data-from="wren">wren</div>',
        }),
      }));

      await app.loadOlderMessages(mockFetchZeroMatch);

      const visibleWithoutBackfill = mockContainer.children.filter((c: any) => c.style.display !== 'none');
      expect(visibleWithoutBackfill.length).toBe(1);

      let calls = 0;
      const mockFetchMulti = jest.fn(async () => {
        calls++;
        if (calls === 1) {
          return {
            ok: true,
            json: async () => ({
              count: 2,
              html: '<div class="msg" id="msg-s2" data-from="silas">silas</div>',
            }),
          };
        }
        return {
          ok: true,
          json: async () => ({
            count: 1,
            html: '<div class="msg" id="msg-a2" data-from="abby-normal">abby backfilled</div>',
          }),
        };
      });

      await app.backfillFilteredHistory(mockFetchMulti, 2);
      const visibleWithBackfill = mockContainer.children.filter((c: any) => c.style.display !== 'none');
      expect(visibleWithBackfill.length).toBe(2);
    });
  });
});
