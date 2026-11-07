// @test-type: unit — actor tiles filter stream and target composer sends (#4732)
// @card: #4732
// @owner: abby-normal
// @domain: messages

import { renderPeopleRow } from '../src/views';
import { ActorRow } from '../src/sources/models';

const app = require('../public/app.js');

describe('#4732 Actor tiles filter stream and target message sends in Clearing v2', () => {
  let mockContainer: any;
  let mockPeopleRow: any;
  let mockTiles: Record<string, any>;
  let mockComposerForm: any;
  let mockComposerInput: any;
  let mockSendError: any;
  let mockSubmitBtn: any;

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
      addEventListener: jest.fn(),
      appendChild(c: any) { this.children.push(c); },
      querySelector: (sel: string) => null,
      querySelectorAll: function(sel: string) { return this.children || []; },
      focus: jest.fn(),
      getAttribute: (name: string) => null,
      setAttribute: jest.fn(),
    };
  }

  beforeEach(() => {
    // Reset filter
    if (app.setRoleFilter) {
      app.setRoleFilter(null);
    }

    mockContainer = createMockElement('messages-stream');
    mockPeopleRow = createMockElement('rooms-people-row');
    mockComposerForm = createMockElement('composer-form', 'form');
    mockComposerInput = createMockElement('composer-input', 'textarea');
    mockComposerInput.placeholder = 'Message #team...';
    mockSendError = createMockElement('send-error');
    mockSubmitBtn = createMockElement('composer-send', 'button');
    mockSubmitBtn.textContent = 'Send';
    mockComposerForm.appendChild(mockSubmitBtn);

    mockTiles = {
      wren: createMockElement('tile-wren', 'a'),
      silas: createMockElement('tile-silas', 'a'),
      kade: createMockElement('tile-kade', 'a'),
      'abby-normal': createMockElement('tile-abby-normal', 'a'),
    };
    mockTiles.wren.dataset.role = 'wren';
    mockTiles.silas.dataset.role = 'silas';
    mockTiles.kade.dataset.role = 'kade';
    mockTiles['abby-normal'].dataset.role = 'abby-normal';

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
        if (id === 'send-error') return mockSendError;
        if (id === 'tile-wren') return mockTiles.wren;
        if (id === 'tile-silas') return mockTiles.silas;
        if (id === 'tile-kade') return mockTiles.kade;
        if (id === 'tile-abby-normal') return mockTiles['abby-normal'];
        return null;
      },
      querySelectorAll: (sel: string) => {
        if (sel.includes('.actor-tile')) return Object.values(mockTiles);
        if (sel.includes('.msg') || sel.includes('.message')) return mockContainer.children;
        return [];
      },
      createElement: (tag: string) => createMockElement('elem-' + Math.random(), tag),
      createTextNode: (t: string) => ({ textContent: t }),
    };

    (global as any).window = {
      CLEARING_BASE: '',
      roomKey: {
        loadOrCreate: () => ({ sk: new Uint8Array(32) }),
        signEvent: (sk: any, kind: any, tags: any, content: string) => ({ id: 'evt-1', pubkey: 'pub1', tags, content }),
      },
    };
  });

  afterEach(() => {
    delete (global as any).document;
    delete (global as any).window;
  });

  describe('AC1: Tapping actor tile toggles filter and hides non-matching messages', () => {
    test('tapping an actor tile filters messages to only those from or mentioning that role', () => {
      // Setup message stream
      const msg1 = createMockElement('msg-1');
      msg1.dataset.from = 'wren';
      msg1.textContent = 'Photos pipeline is ready.';

      const msg2 = createMockElement('msg-2');
      msg2.dataset.from = 'silas';
      msg2.textContent = 'Ontology tests passing.';

      const msg3 = createMockElement('msg-3');
      msg3.dataset.from = 'jeff';
      msg3.textContent = '@kade please run the build now';

      mockContainer.children = [msg1, msg2, msg3];

      // Tap 'kade'
      app.toggleActorFilter(null, 'kade');

      // Tile 'kade' is marked selected / active
      expect(mockTiles.kade.classList.contains('selected')).toBe(true);
      expect(mockTiles.wren.classList.contains('selected')).toBe(false);

      // Stream is filtered: msg3 mentions @kade so it remains visible
      expect(msg3.style.display).toBe('');
      // Non-matching messages are hidden
      expect(msg1.style.display).toBe('none');
      expect(msg2.style.display).toBe('none');

      // Now switch filter to 'wren'
      app.setRoleFilter('wren');
      expect(mockTiles.wren.classList.contains('selected')).toBe(true);
      expect(mockTiles.kade.classList.contains('selected')).toBe(false);

      // msg1 is from wren -> visible; msg2 and msg3 are hidden
      expect(msg1.style.display).toBe('');
      expect(msg2.style.display).toBe('none');
      expect(msg3.style.display).toBe('none');
    });

    test('new live messages appended while filter is active respect the active filter', () => {
      app.toggleActorFilter(null, 'kade');

      // Incoming message from Silas without mentioning kade
      app.appendMessage({ id: 'live-silas', from: 'silas', text: 'Nothing to do', ts: new Date().toISOString(), visible: true });
      const liveSilasMsg = mockContainer.children.find((c: any) => c.id === 'msg-live-silas');
      expect(liveSilasMsg).toBeDefined();
      expect(liveSilasMsg.style.display).toBe('none');

      // Incoming message from Kade
      app.appendMessage({ id: 'live-kade', from: 'kade', text: 'Pull complete', ts: new Date().toISOString(), visible: true });
      const liveKadeMsg = mockContainer.children.find((c: any) => c.id === 'msg-live-kade');
      expect(liveKadeMsg).toBeDefined();
      expect(liveKadeMsg.style.display).toBe('');
    });
  });

  describe('AC2: Composer targeting & thumb-typing shorthand without @<role>', () => {
    test('tapping tile sets composer placeholder and prepends target in input', () => {
      app.toggleActorFilter(null, 'kade');

      expect(mockComposerInput.placeholder).toContain('@kade');
      expect(mockComposerInput.value).toBe('@kade ');
    });

    test('thumb-typing text without @role sends targeted message to active role', async () => {
      app.toggleActorFilter(null, 'kade');

      // Jeff thumb-types "yes card and pull" without leading @kade
      mockComposerInput.value = 'yes card and pull';

      let sentPayload: any = null;
      const mockFetch = jest.fn().mockImplementation(async (url: string, opts: any) => {
        sentPayload = JSON.parse(opts.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage('yes card and pull', {
        inputEl: mockComposerInput,
        btnEl: mockSubmitBtn,
        fetchFn: mockFetch,
      });

      expect(mockFetch).toHaveBeenCalled();
      expect(sentPayload.text).toBe('@kade yes card and pull');
    });

    test('if input already begins with @kade, does not double-prefix', async () => {
      app.toggleActorFilter(null, 'kade');

      let sentPayload: any = null;
      const mockFetch = jest.fn().mockImplementation(async (url: string, opts: any) => {
        sentPayload = JSON.parse(opts.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage('@kade go ahead', {
        inputEl: mockComposerInput,
        btnEl: mockSubmitBtn,
        fetchFn: mockFetch,
      });

      expect(sentPayload.text).toBe('@kade go ahead');
    });
  });

  describe('AC3: Deselection restores full stream and general broadcast', () => {
    test('tapping the active actor tile again deselects it and restores all messages', () => {
      const msg1 = createMockElement('msg-1');
      msg1.dataset.from = 'wren';
      msg1.textContent = 'Photos pipeline is ready.';

      const msg2 = createMockElement('msg-2');
      msg2.dataset.from = 'silas';
      msg2.textContent = 'Ontology tests passing.';

      mockContainer.children = [msg1, msg2];

      // Select wren
      app.toggleActorFilter(null, 'wren');
      expect(mockTiles.wren.classList.contains('selected')).toBe(true);
      expect(msg2.style.display).toBe('none');

      // Deselect wren
      app.toggleActorFilter(null, 'wren');
      expect(mockTiles.wren.classList.contains('selected')).toBe(false);

      // Stream fully restored
      expect(msg1.style.display).toBe('');
      expect(msg2.style.display).toBe('');

      // Composer placeholder reset
      expect(mockComposerInput.placeholder).not.toContain('@wren');
    });

    test('send after deselection broadcasts without prepending role', async () => {
      // Select then deselect
      app.toggleActorFilter(null, 'kade');
      app.toggleActorFilter(null, 'kade');

      let sentPayload: any = null;
      const mockFetch = jest.fn().mockImplementation(async (url: string, opts: any) => {
        sentPayload = JSON.parse(opts.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage('team sync in 5m', {
        inputEl: mockComposerInput,
        btnEl: mockSubmitBtn,
        fetchFn: mockFetch,
      });

      expect(sentPayload.text).toBe('team sync in 5m');
    });
  });

  describe('AC4: NEGATIVE PROOF (#3734)', () => {
    test('NEGATIVE PROOF: unrelated messages mentioning another role are strictly hidden', () => {
      const msgUnrelated = createMockElement('msg-unrelated');
      msgUnrelated.dataset.from = 'silas';
      msgUnrelated.textContent = '@wren please check #4635';

      mockContainer.children = [msgUnrelated];

      // Select 'kade'
      app.toggleActorFilter(null, 'kade');

      // Silas mentioning wren is NOT kade, so must be hidden
      expect(msgUnrelated.style.display).toBe('none');
    });

    test('NEGATIVE PROOF: selecting null or unknown role does not break filtering or wipe composer input', () => {
      mockComposerInput.value = 'hello team';
      app.toggleActorFilter(null, null);

      expect(app.getActiveRoleFilter()).toBeNull();
      // Composer input retained
      expect(mockComposerInput.value).toBe('hello team');
    });

    test('renderPeopleRow generates actor-tile elements with data-role and toggleActorFilter click handler', () => {
      const actors: ActorRow[] = [
        { id: 'wren', name: 'Wren', state: 'building', sessions: ['live'], cards: [] },
        { id: 'kade', name: 'Kade', state: 'building', sessions: ['live'], cards: [] },
      ];

      const html = renderPeopleRow('team', actors);
      expect(html).toContain('data-role="wren"');
      expect(html).toContain('data-role="kade"');
      expect(html).toContain('toggleActorFilter');
    });
  });
});
