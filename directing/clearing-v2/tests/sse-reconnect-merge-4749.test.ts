// @test-type: unit — tests app.js SSE reconnect merge without DOM wipe
// @card: #4749
// @domain: messages

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
    textContent: '',
    innerHTML: '',
    outerHTML: '',
    addEventListener: jest.fn(),
    appendChild(c: any) { this.children.push(c); },
    querySelector(sel: string) {
      if (sel.startsWith('#')) {
        const targetId = sel.slice(1);
        return this.children.find((c: any) => c.id === targetId) || null;
      }
      if (sel === '[id^="msg-"]') {
        return this.children.find((c: any) => c.id && c.id.startsWith('msg-')) || null;
      }
      return null;
    },
    querySelectorAll(sel: string) { return this.children; },
    insertAdjacentHTML(pos: string, html: string) {
      const added = createMockElement('msg-older_0');
      added.textContent = 'older';
      this.children.unshift(added);
    }
  };
  return el;
}

describe('#4749 app.js SSE reconnect and scroll history preservation', () => {
  let mockContainer: any;
  let mockHistoricalMsg: any;
  let mockCurrentMsg: any;
  let mockRow: any;
  let elementsById: Record<string, any>;

  beforeEach(() => {
    elementsById = {};

    mockContainer = createMockElement('messages-stream');
    mockHistoricalMsg = createMockElement('msg-scrolled_1');
    mockHistoricalMsg.textContent = 'historical message';
    mockHistoricalMsg.dataset = { ts: '2026-10-26T07:00:00.000Z', from: 'wren', body: 'historical message' };

    mockCurrentMsg = createMockElement('msg-current_1');
    mockCurrentMsg.textContent = 'recent message';
    mockCurrentMsg.dataset = { ts: '2026-10-26T08:00:00.000Z', from: 'jeff', body: 'recent message' };

    mockContainer.children.push(mockHistoricalMsg, mockCurrentMsg);

    mockRow = createMockElement('rooms-people-row');
    mockRow.outerHTML = '<div id="rooms-people-row"><span>actors</span></div>';

    elementsById['messages-stream'] = mockContainer;
    elementsById['msg-scrolled_1'] = mockHistoricalMsg;
    elementsById['msg-current_1'] = mockCurrentMsg;
    elementsById['rooms-people-row'] = mockRow;
    elementsById['room-picker'] = { value: 'team' };
    elementsById['relay-status'] = createMockElement('relay-status');

    (global as any).document = {
      getElementById: (id: string) => elementsById[id] || null,
      createElement: (tag: string) => createMockElement(`dyn-${Math.random().toString(36).slice(2, 7)}`, tag),
      createTextNode: (text: string) => ({ textContent: text }),
      querySelector: (sel: string) => null,
      querySelectorAll: (sel: string) => [],
    };

    (global as any).window = {
      location: { pathname: '/rooms/team', protocol: 'http:' },
      CLEARING_BASE: '',
    };
  });

  afterEach(() => {
    delete (global as any).document;
    delete (global as any).window;
    delete (global as any).EventSource;
  });

  test('SSE init event preserves prepended/scrolled-back history without wiping container', () => {
    let eventSourceInstance: any;
    (global as any).EventSource = class MockEventSource {
      listeners: Record<string, Function[]> = {};
      constructor() { eventSourceInstance = this; }
      addEventListener(evt: string, fn: Function) {
        this.listeners[evt] = this.listeners[evt] || [];
        this.listeners[evt].push(fn);
      }
      trigger(evt: string, data: any) {
        (this.listeners[evt] || []).forEach((fn) => fn({ data: JSON.stringify(data) }));
      }
    };

    // Ensure messagesContainer references mockContainer
    Object.defineProperty(mockContainer, 'innerHTML', {
      get() { return ''; },
      set(val) {
        if (val === '') {
          this.children = [];
        }
      }
    });

    app.initStream();

    // Trigger SSE init event with only the latest page
    eventSourceInstance.trigger('init', {
      messages: [
        { id: 'current_1', from: 'jeff', text: 'recent message', ts: '2026-10-26T08:00:00.000Z', visible: true },
        { id: 'current_2', from: 'wren', text: 'brand new message', ts: '2026-10-26T08:05:00.000Z', visible: true }
      ]
    });

    // Invariant: scrolled back message must remain in DOM across init reconnect
    expect(mockContainer.children.some((c: any) => c.id === 'msg-scrolled_1')).toBe(true);
  });

  test('loadOlderMessages sends ts from data-ts attribute', async () => {
    let capturedUrl = '';
    const mockFetch = async (url: string) => {
      capturedUrl = url;
      return {
        ok: true,
        json: async () => ({ count: 1, html: '<div id="msg-older_0" class="msg" data-ts="2026-10-26T06:00:00.000Z">older</div>' })
      };
    };

    const count = await app.loadOlderMessages(mockFetch);
    expect(count).toBe(1);
    expect(capturedUrl).toContain('before=scrolled_1');
    expect(capturedUrl).toContain('ts=');
  });

  test('refreshPeopleRow avoids layout shifts when HTML content is unchanged', async () => {
    const initialHtml = mockRow.outerHTML;
    let outerHtmlSetterCount = 0;
    Object.defineProperty(mockRow, 'outerHTML', {
      get() { return initialHtml; },
      set(val) { outerHtmlSetterCount++; }
    });

    const mockFetch = async () => ({
      ok: true,
      json: async () => ({ html: initialHtml })
    });

    await app.refreshPeopleRow(mockFetch);
    // Unchanged HTML must not trigger outerHTML assignment
    expect(outerHtmlSetterCount).toBe(0);
  });

  test('loadOlderMessages surfaces "couldn\'t load older" when fetch fails with non-ok status', async () => {
    const banner = createMockElement('send-error');
    elementsById['send-error'] = banner;

    const mockFailingFetch = async () => ({
      ok: false,
      status: 502,
      json: async () => ({ ok: false, error: 'relay disconnected' })
    });

    const count = await app.loadOlderMessages(mockFailingFetch);
    expect(count).toBe(0);
    expect(banner.textContent).toBe("couldn't load older");
  });
});
