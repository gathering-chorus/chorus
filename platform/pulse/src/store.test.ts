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

describe('Nudge delivery', () => {
  test('creates a nudge with a positive id', () => {
    const id = store.sendNudge('silas', 'wren', 'test nudge');
    expect(id).toBeGreaterThan(0);
  });

  test('recipient has exactly one pending nudge after send', () => {
    store.sendNudge('silas', 'wren', 'test nudge');
    const pending = store.getPendingNudges('wren');
    expect(pending).toHaveLength(1);
    expect(pending[0].content).toBe('test nudge');
    expect(pending[0].from).toBe('silas');
  });

  test('non-recipient has zero pending', () => {
    store.sendNudge('silas', 'wren', 'test nudge');
    expect(store.getPendingNudges('silas')).toHaveLength(0);
  });

  test('acknowledge clears pending', () => {
    const id = store.sendNudge('silas', 'wren', 'test nudge');
    store.acknowledgeNudge(id);
    expect(store.getPendingNudges('wren')).toHaveLength(0);
  });
});

describe('Multiple nudges + ack-all', () => {
  test('three pending, ack-all returns 3, pending cleared', () => {
    store.sendNudge('wren', 'kade', 'nudge 1');
    store.sendNudge('silas', 'kade', 'nudge 2');
    store.sendNudge('wren', 'kade', 'nudge 3');
    expect(store.getPendingNudges('kade')).toHaveLength(3);

    const acked = store.acknowledgeAllNudges('kade');
    expect(acked).toBe(3);
    expect(store.getPendingNudges('kade')).toHaveLength(0);
  });
});

describe('Dead letter', () => {
  test('third delivery attempt dead-letters the nudge', () => {
    const id = store.sendNudge('silas', 'wren', 'will fail');
    expect(store.recordDeliveryAttempt(id).deadLettered).toBe(false);
    expect(store.recordDeliveryAttempt(id).deadLettered).toBe(false);
    expect(store.recordDeliveryAttempt(id).deadLettered).toBe(true);

    expect(store.getPendingNudges('wren')).toHaveLength(0);
    const dl = store.getDeadLetters();
    expect(dl).toHaveLength(1);
    expect(dl[0].delivery_attempts).toBe(3);
  });

  test('replay restores the nudge to pending and clears dead-letter', () => {
    const id = store.sendNudge('silas', 'wren', 'will fail');
    store.recordDeliveryAttempt(id);
    store.recordDeliveryAttempt(id);
    store.recordDeliveryAttempt(id);

    store.replayDeadLetter(id);
    expect(store.getDeadLetters()).toHaveLength(0);
    expect(store.getPendingNudges('wren')).toHaveLength(1);
  });
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

describe('Role state', () => {
  test('setRoleState records state and card', () => {
    store.setRoleState('silas', 'building', '1755');
    const state = store.getRoleState('silas');
    expect(state?.state).toBe('building');
    expect(state?.card).toBe('1755');
  });

  test('transitioning to idle clears the card', () => {
    store.setRoleState('silas', 'building', '1755');
    store.setRoleState('silas', 'idle');
    const updated = store.getRoleState('silas');
    expect(updated?.state).toBe('idle');
    expect(updated?.card).toBeNull();
  });
});

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
