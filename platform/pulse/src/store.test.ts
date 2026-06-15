/* eslint-disable @typescript-eslint/no-explicit-any -- test reads dynamic sqlite row shapes; `any[]` casts keep the assertions terse (#3429) */
/**
 * Messaging Store Tests (#1755, migrated to jest in #2154)
 * Nudge, chat, dead-letter delivery paths.
 *
 * Each test gets a fresh DB via beforeEach; afterEach closes the store and
 * unlinks the file so failures don't leave artifacts on disk.
 */

import { MessageStore, inferNudgeClass } from './store';
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

describe('Nudge envelope (#3403 — class r2r/a2r + expects)', () => {
  test('migration adds nudge_class + nudge_expects; sendNudge carries them', () => {
    expect(MessageStore.prototype.sendNudge).toBeDefined();
    store.sendNudge('silas', 'wren', 'review my card', undefined, 'r2r', 'reply');
    const rows = store.queryMessages({ type: 'nudge', to: 'wren' }) as any[];
    expect(rows).toHaveLength(1);
    expect(rows[0].nudge_class).toBe('r2r');
    expect(rows[0].nudge_expects).toBe('reply');
  });

  test('a peer fyi is r2r/none; a system alert is a2r/none — neither shape can over-trap', () => {
    expect(MessageStore.prototype.sendNudge).toBeDefined();
    store.sendNudge('kade', 'wren', 'heads up', undefined, 'r2r', 'none');
    store.sendNudge('system', 'wren', 'eventloop blocked', undefined, 'a2r', 'none');
    const rows = store.queryMessages({ type: 'nudge', to: 'wren' }) as any[];
    const peer = rows.find(r => r.from === 'kade');
    const alert = rows.find(r => r.from === 'system');
    expect(peer.nudge_class).toBe('r2r');
    expect(peer.nudge_expects).toBe('none');
    expect(alert.nudge_class).toBe('a2r');
    expect(alert.nudge_expects).toBe('none');
  });

  test('inferNudgeClass: peers are r2r, machine/alert senders are a2r', () => {
    expect(inferNudgeClass('wren')).toBe('r2r');
    expect(inferNudgeClass('silas')).toBe('r2r');
    expect(inferNudgeClass('kade')).toBe('r2r');
    expect(inferNudgeClass('jeff')).toBe('r2r');
    expect(inferNudgeClass('system')).toBe('a2r');
    expect(inferNudgeClass('chorus-mcp')).toBe('a2r');
    expect(inferNudgeClass('pulse')).toBe('a2r');
  });

  test('defaults are safe: bare sendNudge is r2r/none (cannot trap until expects is set)', () => {
    expect(MessageStore.prototype.queryMessages).toBeDefined();
    store.sendNudge('silas', 'wren', 'no envelope given');
    const rows = store.queryMessages({ type: 'nudge', to: 'wren' }) as any[];
    expect(rows[0].nudge_class).toBe('r2r');
    expect(rows[0].nudge_expects).toBe('none');
  });
});

describe('Delivery columns (#2727 AC1)', () => {
  test('sendNudge places row in pending; getPendingDeliveries returns it', () => {
    expect(MessageStore.prototype.getPendingDeliveries).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'test');
    const pending = store.getPendingDeliveries();
    expect(pending.map(r => r.id)).toContain(id);
  });

  test('markDelivered removes row from getPendingDeliveries', () => {
    expect(MessageStore.prototype.markDelivered).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'test');
    store.markDelivered(id);
    const pending = store.getPendingDeliveries();
    expect(pending.map(r => r.id)).not.toContain(id);
  });

  test('markFailed removes row from getPendingDeliveries', () => {
    expect(MessageStore.prototype.markFailed).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'test');
    store.markFailed(id, 'tcc-denied');
    const pending = store.getPendingDeliveries();
    expect(pending.map(r => r.id)).not.toContain(id);
  });

  test('markFailed records last_delivery_error retrievable via getDeliveryRecord', () => {
    expect(MessageStore.prototype.getDeliveryRecord).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'test');
    store.markFailed(id, 'tcc-denied');
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('failed');
    expect(rec.last_delivery_error).toBe('tcc-denied');
  });

  test('markDelivered records delivered_at retrievable via getDeliveryRecord', () => {
    expect(MessageStore.prototype.markDelivered).toBeDefined();
    const id = store.sendNudge('silas', 'wren', 'test');
    store.markDelivered(id);
    const rec = store.getDeliveryRecord(id);
    expect(rec.delivery_status).toBe('delivered');
    expect(rec.delivered_at).not.toBeNull();
  });

  test('getPendingDeliveries returns oldest-first', () => {
    expect(MessageStore.prototype.getPendingDeliveries).toBeDefined();
    const a = store.sendNudge('silas', 'wren', 'a');
    const b = store.sendNudge('silas', 'wren', 'b');
    const c = store.sendNudge('silas', 'wren', 'c');
    store.markDelivered(b);
    const pending = store.getPendingDeliveries();
    expect(pending.map(r => r.id)).toEqual([a, c]);
  });

  test('migration idempotent — close, reopen, queryMessages still surfaces existing nudges', () => {
    const id = store.sendNudge('silas', 'wren', 'test');
    store.close();
    const reopened = new MessageStore(TEST_DB);
    const rows = reopened.queryMessages({ type: 'nudge', to: 'wren' });
    expect(rows.find(r => r.id === id)?.content).toBe('test');
    reopened.close();
    store = reopened;
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

// #3343 — jeff-input rows: raw content, distinct type, requeue-visible.
describe('#3343 sendJeffInput', () => {
  test('stores raw content with type jeff-input from jeff', () => {
    const id = store.sendJeffInput('wren', 'do the thing', 'trace-ji-1');
    expect(id).toBeGreaterThan(0);
    const pending = store.getPendingDeliveries().find(r => r.id === id);
    expect(pending).toBeDefined();
    expect(pending!.kind).toBe('jeff-input');
    expect(pending!.from).toBe('jeff');
    expect(pending!.content).toBe('do the thing'); // RAW — no [nudge from] framing
    expect(pending!.trace_id).toBe('trace-ji-1');
  });

  test('getPendingDeliveries returns both kinds, each correctly labeled', () => {
    store.sendNudge('silas', 'wren', 'a nudge');
    store.sendJeffInput('wren', 'jeff words');
    const kinds = store.getPendingDeliveries().map(r => r.kind).sort();
    expect(kinds).toEqual(['jeff-input', 'nudge']);
  });
});

// #3343 — the LIVE messages.db carries the pre-#3343 CHECK constraint; opening
// it must rebuild the table once (idempotent) and preserve existing rows.
describe('#3343 CHECK-constraint rebuild migration', () => {
  const OLD_DB = path.join(__dirname, '..', 'test-old-schema.db');

  afterEach(() => {
    if (fs.existsSync(OLD_DB)) fs.unlinkSync(OLD_DB);
  });

  test('opening a DB with the old CHECK rebuilds it, preserves rows, and accepts jeff-input', () => {
    // Build a DB with the OLD schema (no jeff-input in CHECK) + one nudge row.
    const Database = require('better-sqlite3');
    const raw = new Database(OLD_DB);
    raw.exec(`
      CREATE TABLE messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT NOT NULL CHECK(type IN ('nudge', 'chat', 'board-event', 'role-state')),
        "from" TEXT NOT NULL, "to" TEXT NOT NULL, content TEXT NOT NULL,
        chat_id TEXT, acknowledged INTEGER DEFAULT 0, delivery_attempts INTEGER DEFAULT 0,
        dead_letter INTEGER DEFAULT 0, created_at TEXT DEFAULT (datetime('now')),
        acknowledged_at TEXT, dead_lettered_at TEXT
      );
      INSERT INTO messages (type, "from", "to", content) VALUES ('nudge', 'silas', 'wren', 'pre-migration row');
    `);
    raw.close();

    const migrated = new MessageStore(OLD_DB);
    // old row survives
    const rec = migrated.getDeliveryRecord(1);
    expect(rec.delivery_status).toBe('delivered'); // #2727 backfill applies to pre-worker rows
    // jeff-input now insertable (old CHECK would have thrown)
    const id = migrated.sendJeffInput('wren', 'post-migration jeff words');
    expect(id).toBeGreaterThan(1);
    // idempotent: reopening does not rebuild again / lose anything
    migrated.close();
    const reopened = new MessageStore(OLD_DB);
    expect(reopened.getPendingDeliveries().find(r => r.id === id)!.content).toBe('post-migration jeff words');
    reopened.close();
  });
});
