// @test-type: unit — actor tiles 0..n multi-selection filtering and targeting (#4736)
// @card: #4736
// @owner: abby-normal
// @domain: messages

import { renderPeopleRow } from '../src/views';
import { ActorRow } from '../src/sources/models';

const app = require('../public/app.js');

describe('#4736 Actor tiles 0..n multi-select in Clearing v2', () => {
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
    // Reset filters
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
        if (id === 'send-error') return mockSendError;
        if (id === 'tile-wren') return mockTiles.wren;
        if (id === 'tile-silas') return mockTiles.silas;
        if (id === 'tile-kade') return mockTiles.kade;
        if (id === 'tile-abby-normal') return mockTiles['abby-normal'];
        if (id === 'tile-borg-queen') return mockTiles['borg-queen'];
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

  describe('AC1: 0..n Actor multi-selection toggle', () => {
    test('tapping multiple actor tiles selects all tapped roles concurrently', () => {
      expect(app.getActiveRoleFilters ? app.getActiveRoleFilters() : []).toEqual([]);

      // Tap 'wren'
      app.toggleActorFilter(null, 'wren');
      expect(mockTiles.wren.classList.contains('selected')).toBe(true);
      expect(mockTiles.kade.classList.contains('selected')).toBe(false);
      expect(app.getActiveRoleFilters()).toEqual(['wren']);

      // Tap 'kade' -> now BOTH 'wren' and 'kade' are selected (2 actors)
      app.toggleActorFilter(null, 'kade');
      expect(mockTiles.wren.classList.contains('selected')).toBe(true);
      expect(mockTiles.kade.classList.contains('selected')).toBe(true);
      expect(mockTiles.silas.classList.contains('selected')).toBe(false);
      expect(app.getActiveRoleFilters().sort()).toEqual(['kade', 'wren']);

      // Tap 'wren' again -> deselects wren, leaves only kade (1 actor)
      app.toggleActorFilter(null, 'wren');
      expect(mockTiles.wren.classList.contains('selected')).toBe(false);
      expect(mockTiles.kade.classList.contains('selected')).toBe(true);
      expect(app.getActiveRoleFilters()).toEqual(['kade']);

      // Tap 'kade' again -> deselects kade (0 actors selected)
      app.toggleActorFilter(null, 'kade');
      expect(mockTiles.wren.classList.contains('selected')).toBe(false);
      expect(mockTiles.kade.classList.contains('selected')).toBe(false);
      expect(app.getActiveRoleFilters()).toEqual([]);
    });
  });

  describe('AC2: Stream filtering with 0..n actors', () => {
    test('stream shows messages from ANY selected actor and hides unselected ones', () => {
      const msgWren = createMockElement('msg-1');
      msgWren.dataset.from = 'wren';
      msgWren.textContent = 'Design ready.';

      const msgKade = createMockElement('msg-2');
      msgKade.dataset.from = 'kade';
      msgKade.textContent = 'PR open.';

      const msgSilas = createMockElement('msg-3');
      msgSilas.dataset.from = 'silas';
      msgSilas.textContent = 'Database pragma configured.';

      const msgBorg = createMockElement('msg-4');
      msgBorg.dataset.from = 'borg-queen';
      msgBorg.textContent = '@wren verifying prod now.';

      mockContainer.children = [msgWren, msgKade, msgSilas, msgBorg];

      // Select wren and kade
      app.toggleActorFilter(null, 'wren');
      app.toggleActorFilter(null, 'kade');

      // msgWren (from wren) -> visible
      expect(msgWren.style.display).toBe('');
      // msgKade (from kade) -> visible
      expect(msgKade.style.display).toBe('');
      // msgBorg (mentions @wren) -> visible
      expect(msgBorg.style.display).toBe('');
      // msgSilas (from silas, no wren/kade mention) -> hidden
      expect(msgSilas.style.display).toBe('none');
    });

    test('with 0 actors selected, all messages are displayed', () => {
      const msg1 = createMockElement('msg-1');
      msg1.dataset.from = 'wren';
      const msg2 = createMockElement('msg-2');
      msg2.dataset.from = 'silas';
      mockContainer.children = [msg1, msg2];

      // Ensure 0 selected
      app.setRoleFilter(null);

      expect(msg1.style.display).toBe('');
      expect(msg2.style.display).toBe('');
    });
  });

  describe('AC3: Composer targeting with 0..n actors', () => {
    test('composer placeholder reflects 0, 1, or N selected actors', () => {
      // 0 selected
      app.setRoleFilter(null);
      expect(mockComposerInput.placeholder).toContain('team');

      // 1 selected
      app.toggleActorFilter(null, 'wren');
      expect(mockComposerInput.placeholder).toBe('Message @wren...');

      // 2 selected
      app.toggleActorFilter(null, 'kade');
      expect(mockComposerInput.placeholder).toBe('Message @wren, @kade...');

      // 3 selected
      app.toggleActorFilter(null, 'silas');
      expect(mockComposerInput.placeholder).toBe('Message @wren, @kade, @silas...');
    });

    test('thumb-typing without leading @ targets all currently selected actors', async () => {
      // Select wren and kade
      app.toggleActorFilter(null, 'wren');
      app.toggleActorFilter(null, 'kade');

      mockComposerInput.value = 'please sync the schema';

      let sentPayload: any = null;
      const mockFetch = jest.fn().mockImplementation(async (url: string, opts: any) => {
        sentPayload = JSON.parse(opts.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage(mockComposerInput.value, {
        inputEl: mockComposerInput,
        btnEl: mockSubmitBtn,
        statusEl: mockSendError,
        fetchFn: mockFetch,
      });

      expect(mockFetch).toHaveBeenCalled();
      expect(sentPayload).toBeDefined();
      expect(sentPayload.text).toBe('@wren @kade please sync the schema');
    });

    test('with 0 actors selected, untargeted message broadcasts without @ tag', async () => {
      app.setRoleFilter(null);
      mockComposerInput.value = 'hello everyone';

      let sentPayload: any = null;
      const mockFetch = jest.fn().mockImplementation(async (url: string, opts: any) => {
        sentPayload = JSON.parse(opts.body);
        return { ok: true, json: async () => ({ ok: true, stored: true }) };
      });

      await app.handleSendMessage(mockComposerInput.value, {
        inputEl: mockComposerInput,
        btnEl: mockSubmitBtn,
        statusEl: mockSendError,
        fetchFn: mockFetch,
      });

      expect(sentPayload.text).toBe('hello everyone');
    });
  });

  describe('AC4: NEGATIVE PROOFS (#3734)', () => {
    test('NEGATIVE PROOF: with multi-selection active, unselected third party conversations are hidden', () => {
      const msgUnselected = createMockElement('msg-unselected');
      msgUnselected.dataset.from = 'silas';
      msgUnselected.textContent = '@abby-normal check the hook logs';

      mockContainer.children = [msgUnselected];

      // Select 'wren' and 'kade'
      app.toggleActorFilter(null, 'wren');
      app.toggleActorFilter(null, 'kade');

      // Silas mentioning abby-normal matches NEITHER wren NOR kade -> strictly hidden
      expect(msgUnselected.style.display).toBe('none');
    });

    test('NEGATIVE PROOF: deselecting all actors restores visibility to previously hidden messages', () => {
      const msgUnselected = createMockElement('msg-unselected');
      msgUnselected.dataset.from = 'silas';
      msgUnselected.textContent = '@abby-normal check the hook logs';
      mockContainer.children = [msgUnselected];

      // Select wren
      app.toggleActorFilter(null, 'wren');
      expect(msgUnselected.style.display).toBe('none');

      // Deselect wren -> 0 selected
      app.toggleActorFilter(null, 'wren');
      expect(app.getActiveRoleFilters().length).toBe(0);
      expect(msgUnselected.style.display).toBe('');
    });
  });
});
