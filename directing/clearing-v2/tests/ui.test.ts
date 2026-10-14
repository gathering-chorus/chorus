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

  test('formatMessageTime formats valid ISO timestamp', () => {
    const router = new MessageRouter();
    const formatted = formatMessageTime('2026-10-14T14:00:00.000Z');
    expect(typeof formatted).toBe('string');
    expect(formatted.length).toBeGreaterThan(0);
    expect(router.getRecent(1)).toEqual([]);
  });
});
