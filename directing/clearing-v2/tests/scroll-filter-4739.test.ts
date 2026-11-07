// @test-type: unit — Clearing v2 history scroll respects active actor filter and 1-on-1 dialogue (#4739)
// @card: #4739
// @owner: abby-normal
// @domain: messages

const app = require('../public/app.js');

describe('#4739 Clearing v2 history scroll respects active actor filter and 1-on-1 dialogue', () => {
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
        // Mock parser that splits and adds mock elements to container children
        // Simulating inserting before `first`
      },
      focus: jest.fn(),
    };
  }

  beforeEach(() => {
    if (app.setRoleFilter) {
      app.setRoleFilter(null);
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
    };
  });

  afterEach(() => {
    delete (global as any).document;
    delete (global as any).window;
  });

  describe('AC1: Scrolling up in messages-stream applies the active role filter to all prepended history elements immediately', () => {
    test('loadOlderMessages immediately hides unselected roles in prepended history', async () => {
      // Current message in stream
      const currentMsg = createMockElement('msg-100');
      currentMsg.dataset.from = 'abby-normal';
      currentMsg.textContent = 'Here is the current status.';
      mockContainer.children = [currentMsg];

      // Filter on abby-normal
      app.toggleActorFilter(null, 'abby-normal');
      expect(currentMsg.style.display).toBe('');

      // Older messages to prepend
      const oldSilas = createMockElement('msg-98');
      oldSilas.dataset.from = 'silas';
      oldSilas.textContent = 'Silas log entry.';

      const oldJeff = createMockElement('msg-99');
      oldJeff.dataset.from = 'jeff';
      oldJeff.textContent = 'what is ur card status';

      // Mock insertAdjacentHTML to add old messages to mockContainer children
      currentMsg.insertAdjacentHTML = jest.fn((where: string, html: string) => {
        mockContainer.children.unshift(oldSilas, oldJeff);
        mockContainer.scrollHeight = 1800;
      });

      const mockFetch = jest.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          count: 2,
          html: '<div id="msg-98" data-from="silas">Silas log entry.</div><div id="msg-99" data-from="jeff">what is ur card status</div>',
        }),
      });

      const count = await app.loadOlderMessages(mockFetch);
      expect(count).toBe(2);

      // Verify that after prepend, active filter was applied immediately:
      // oldSilas is hidden
      expect(oldSilas.style.display).toBe('none');
      // oldJeff is visible
      expect(oldJeff.style.display).toBe('');
      // currentMsg is visible
      expect(currentMsg.style.display).toBe('');
    });
  });

  describe('AC2: 1-on-1 dialogue preservation ("only me and u")', () => {
    test('when filtered on abby-normal, shows Jeff and Abby Normal and suppresses third party bots/peers', () => {
      const msgJeffPrompt = createMockElement('msg-1');
      msgJeffPrompt.dataset.from = 'jeff';
      msgJeffPrompt.textContent = 'what is ur card status';

      const msgAbbyReply = createMockElement('msg-2');
      msgAbbyReply.dataset.from = 'abby-normal';
      msgAbbyReply.textContent = 'Card #4739 is in WIP';

      const msgBorgQueen = createMockElement('msg-3');
      msgBorgQueen.dataset.from = 'borg-queen';
      msgBorgQueen.textContent = '@abby-normal #4736 proved in prod and accepted';

      const msgWrenNote = createMockElement('msg-4');
      msgWrenNote.dataset.from = 'wren';
      msgWrenNote.textContent = 'On your model.rs note: the tree is derived in memory';

      const msgJeffToKade = createMockElement('msg-5');
      msgJeffToKade.dataset.from = 'jeff';
      msgJeffToKade.textContent = '@kade please run the build';

      mockContainer.children = [msgJeffPrompt, msgAbbyReply, msgBorgQueen, msgWrenNote, msgJeffToKade];

      // Filter on abby-normal
      app.toggleActorFilter(null, 'abby-normal');

      // "only me and u"
      expect(msgJeffPrompt.style.display).toBe('');
      expect(msgAbbyReply.style.display).toBe('');

      // Third party messages suppressed
      expect(msgBorgQueen.style.display).toBe('none');
      expect(msgWrenNote.style.display).toBe('none');

      // Jeff prompt targeting other role suppressed
      expect(msgJeffToKade.style.display).toBe('none');
    });

    test('isMessageVisibleWithFilters handles broadcast mentions and untargeted prompts', () => {
      app.setRoleFilter('abby-normal');

      // Untargeted prompt from Jeff
      expect(app.isMessageVisibleWithFilters('jeff', 'done')).toBe(true);
      expect(app.isMessageVisibleWithFilters('jeff', 'does that mean its landing or queued?')).toBe(true);

      // Targeted to active role
      expect(app.isMessageVisibleWithFilters('jeff', '@abby-normal check this')).toBe(true);

      // Broadcast to @team
      expect(app.isMessageVisibleWithFilters('jeff', '@team all hands')).toBe(true);

      // Targeted to unselected role
      expect(app.isMessageVisibleWithFilters('jeff', '@silas check database')).toBe(false);

      // From active role
      expect(app.isMessageVisibleWithFilters('abby-normal', 'Working on tests')).toBe(true);

      // From unselected third party
      expect(app.isMessageVisibleWithFilters('kade', 'Done with build')).toBe(false);
      expect(app.isMessageVisibleWithFilters('borg-queen', 'Alert: cicd passed')).toBe(false);
    });
  });

  describe('AC3: NEGATIVE PROOFS (#3734)', () => {
    test('NEGATIVE PROOF: history prepend without applyRoleFilters leaves unselected messages exposed', () => {
      const unselectedMsg = createMockElement('msg-unselected');
      unselectedMsg.dataset.from = 'kade';
      unselectedMsg.textContent = 'Running compiler in background';
      unselectedMsg.style.display = '';

      mockContainer.children = [unselectedMsg];

      // With filter on abby-normal, running applyRoleFilters hides unselectedMsg
      app.toggleActorFilter(null, 'abby-normal');
      expect(unselectedMsg.style.display).toBe('none');

      // Violate condition: if an element is prepended without applyRoleFilters, it remains visible (leaking)
      const leakedMsg = createMockElement('msg-leaked');
      leakedMsg.dataset.from = 'silas';
      leakedMsg.textContent = 'Raw prepended HTML';
      leakedMsg.style.display = ''; // raw un-styled element from server HTML
      mockContainer.children.push(leakedMsg);

      // Proof: without applyRoleFilters, display is '' (violation!)
      expect(leakedMsg.style.display).toBe('');

      // Applying role filter fixes it and proves the check is active
      app.applyRoleFilters();
      expect(leakedMsg.style.display).toBe('none');
    });

    test('NEGATIVE PROOF: third-party message mentioning active role is NOT displayed in 1-on-1 dialogue', () => {
      app.toggleActorFilter(null, 'abby-normal');

      // A third-party peer mentions @abby-normal
      const peerMsg = createMockElement('msg-peer');
      peerMsg.dataset.from = 'wren';
      peerMsg.textContent = '@abby-normal design specs attached';
      mockContainer.children = [peerMsg];

      app.applyRoleFilters();

      // Proves strictly hidden under "only me and u" requirement
      expect(peerMsg.style.display).toBe('none');
    });
  });
});
