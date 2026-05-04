/**
 * Messaging Store Tests (#1755, migrated to jest in #2154)
 * Nudge, chat, dead-letter delivery paths.
 *
 * Each test gets a fresh DB via beforeEach; afterEach closes the store and
 * unlinks the file so failures don't leave artifacts on disk.
 */

import { MessageStore } from './store';
import * as fs from 'fs';
import * as path from 'path';

const TEST_DB = path.join(__dirname, '..', 'test-messages.db');

let store: MessageStore;

beforeEach(() => {
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
  store = new MessageStore(TEST_DB);
});

afterEach(() => {
  try { store.close(); } catch { /* already closed */ }
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
});

describe('Nudge persist (canonical path — DEC-107)', () => {
  test('sendNudge writes to messages.db and returns positive id', () => {
    const id = store.sendNudge('silas', 'wren', 'test nudge');
    expect(id).toBeGreaterThan(0);
  });

  test('queryMessages surfaces persisted nudges by recipient (#2664: replaces getPendingNudges read path)', () => {
    store.sendNudge('silas', 'wren', 'test nudge');
    const rows = store.queryMessages({ type: 'nudge', to: 'wren' });
    expect(rows).toHaveLength(1);
    expect(rows[0].content).toBe('test nudge');
    expect(rows[0].from).toBe('silas');
  });

  // #2664: getPendingNudges, acknowledgeNudge, acknowledgeAllNudges,
  // recordDeliveryAttempt, getDeadLetters, replayDeadLetter retired.
  // Delivery confirmation is the nudge.surfaced spine event, not a
  // store-side ack/attempt cycle. The acknowledged / delivery_attempts
  // / dead_letter columns are vestigial — write-once-zero — until a
  // separate column-drop migration card lands.
});

describe('Chat', () => {
  test('chat id is namespaced by the participant pair', () => {
    const chatId = store.startChat('silas', 'kade', 'test topic');
    expect(chatId).toContain('silas-kade');
  });

  test('messages append in order and are retrievable', () => {
    const chatId = store.startChat('silas', 'kade', 'test topic');
    store.chatMessage(chatId, 'silas', 'hello kade');
    store.chatMessage(chatId, 'kade', 'hello silas');
    store.chatMessage(chatId, 'silas', 'how are you');

    const msgs = store.getChatMessages(chatId);
    expect(msgs).toHaveLength(3);
    expect(msgs[0].from).toBe('silas');
    expect(msgs[1].from).toBe('kade');
  });

  test('since cursor returns messages after the given id', () => {
    const chatId = store.startChat('silas', 'kade', 'test topic');
    store.chatMessage(chatId, 'silas', 'hello kade');
    store.chatMessage(chatId, 'kade', 'hello silas');
    store.chatMessage(chatId, 'silas', 'how are you');

    const msgs = store.getChatMessages(chatId);
    const since = store.getChatMessages(chatId, msgs[1].id);
    expect(since).toHaveLength(1);
    expect(since[0].from).toBe('silas');
  });
});

// #2632: 'Role state' describe block retired. setRoleState/getRoleState
// were retired alongside the HTTP role-state writer (zero callers,
// parallel to chorus-hook-shim CLI). Tests for the retired surface go
// with the surface — no @skip-tagged keepers.

describe('Query', () => {
  test('queryMessages filters by recipient and sender', () => {
    store.sendNudge('silas', 'wren', 'nudge A');
    store.sendNudge('kade', 'wren', 'nudge B');
    store.sendNudge('silas', 'kade', 'nudge C');

    expect(store.queryMessages({ to: 'wren' })).toHaveLength(2);
    expect(store.queryMessages({ from: 'silas' })).toHaveLength(2);
  });

  test('stats reflects total and pending counts', () => {
    store.sendNudge('silas', 'wren', 'nudge A');
    store.sendNudge('kade', 'wren', 'nudge B');
    store.sendNudge('silas', 'kade', 'nudge C');

    const stats = store.getStats();
    expect(stats.total).toBe(3);
    expect(stats.pending).toBe(3);
  });
});
