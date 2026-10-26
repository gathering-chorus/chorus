// @test-type: unit — AC4 search & filter combining actor filter with loaded message search and live SSE
// @card: #4749
// @domain: messages

export {};
const app = require('../public/app.js');

function createMockElement(id: string, tag = 'div') {
  const classListSet = new Set<string>();
  const el: any = {
    id,
    tagName: tag.toUpperCase(),
    className: '',
    classList: {
      add: (c: string) => classListSet.add(c),
      remove: (c: string) => classListSet.delete(c),
      contains: (c: string) => classListSet.has(c),
    },
    style: { display: '' },
    dataset: {} as Record<string, string>,
    children: [] as any[],
    value: '',
    textContent: '',
    innerText: '',
    innerHTML: '',
    outerHTML: '',
    addEventListener: jest.fn(),
    appendChild(c: any) { this.children.push(c); },
    querySelector(sel: string) {
      if (sel.startsWith('#')) return this.children.find((c: any) => c.id === sel.slice(1)) || null;
      if (sel.includes('.message-author')) return this.children.find((c: any) => c.className?.includes('message-author')) || null;
      return null;
    },
    querySelectorAll(sel: string) { return this.children; },
  };
  return el;
}

describe('#4749 AC4: stream-grade search and filter over loaded messages', () => {
  let stream: any;
  let searchInput: any;

  beforeEach(() => {
    stream = createMockElement('messages-stream');
    searchInput = createMockElement('room-search-input', 'input');

    (global as any).document = {
      getElementById: (id: string) => {
        if (id === 'messages-stream') return stream;
        if (id === 'room-search-input') return searchInput;
        return null;
      },
      createElement: (tag: string) => createMockElement(`temp-${Math.random()}`, tag),
      createTextNode: (text: string) => ({ textContent: text }),
    };

    app.setRoleFilter(null);
  });

  afterEach(() => {
    app.setRoleFilter(null);
    delete (global as any).document;
  });

  test('RED PROOF: search query hides loaded messages not matching text query', () => {
    // Message 1 from wren mentioning pulse
    const msg1 = createMockElement('msg-1');
    msg1.textContent = 'working on pulse door';
    msg1.dataset = { from: 'wren', body: 'working on pulse door' };

    // Message 2 from wren mentioning something else
    const msg2 = createMockElement('msg-2');
    msg2.textContent = 'deploying images api';
    msg2.dataset = { from: 'wren', body: 'deploying images api' };

    stream.children.push(msg1, msg2);

    // Apply search query "pulse"
    app.setSearchQuery('pulse');

    expect(msg1.style.display).toBe('');
    expect(msg2.style.display).toBe('none');
  });

  test('RED PROOF: search query combines with active actor tile filter', () => {
    // Message 1: wren talking about pulse
    const msg1 = createMockElement('msg-1');
    msg1.textContent = 'wren: pulse update';
    msg1.dataset = { from: 'wren', body: 'pulse update' };

    // Message 2: silas talking about pulse
    const msg2 = createMockElement('msg-2');
    msg2.textContent = 'silas: pulse config';
    msg2.dataset = { from: 'silas', body: 'pulse config' };

    stream.children.push(msg1, msg2);

    // Filter to wren only AND search "pulse"
    app.setRoleFilter('wren');
    app.setSearchQuery('pulse');

    expect(msg1.style.display).toBe('');
    expect(msg2.style.display).toBe('none'); // silas matches search query but fails actor filter
  });

  test('live SSE append respects both search query and actor filter', () => {
    app.setRoleFilter('wren');
    app.setSearchQuery('pipeline');

    // Live message from wren matching "pipeline"
    app.appendMessage({ id: 'live-1', from: 'wren', text: 'checking pipeline', visible: true });
    const live1 = stream.children.find((c: any) => c.id === 'msg-live-1');
    expect(live1).toBeDefined();
    expect(live1.style.display).not.toBe('none');

    // Live message from wren NOT matching "pipeline"
    app.appendMessage({ id: 'live-2', from: 'wren', text: 'breakfast time', visible: true });
    const live2 = stream.children.find((c: any) => c.id === 'msg-live-2');
    expect(live2).toBeDefined();
    expect(live2.style.display).toBe('none');

    // Live message from silas matching "pipeline" (fails actor filter)
    app.appendMessage({ id: 'live-3', from: 'silas', text: 'pipeline done', visible: true });
    const live3 = stream.children.find((c: any) => c.id === 'msg-live-3');
    expect(live3).toBeDefined();
    expect(live3.style.display).toBe('none');
  });
});
