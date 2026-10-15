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
});
