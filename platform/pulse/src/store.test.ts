/**
 * Messaging Store Tests (#1755)
 * Nudge, chat, dead-letter delivery paths
 */

import { MessageStore } from './store';
import * as fs from 'fs';
import * as path from 'path';

const TEST_DB = path.join(__dirname, '..', 'test-messages.db');

function freshStore(): MessageStore {
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
  return new MessageStore(TEST_DB);
}

let pass = 0;
let fail = 0;

function check(name: string, result: boolean): void {
  if (result) { console.log(`  PASS: ${name}`); pass++; }
  else { console.log(`  FAIL: ${name}`); fail++; }
}

// --- Nudge Tests ---
console.log('--- Nudge delivery ---');
{
  const store = freshStore();
  const id = store.sendNudge('silas', 'wren', 'test nudge');
  check('Nudge created with ID', id > 0);

  const pending = store.getPendingNudges('wren');
  check('Wren has 1 pending nudge', pending.length === 1);
  check('Nudge content correct', pending[0].content === 'test nudge');
  check('Nudge from silas', pending[0].from === 'silas');

  const silasPending = store.getPendingNudges('silas');
  check('Silas has 0 pending', silasPending.length === 0);

  store.acknowledgeNudge(id);
  const afterAck = store.getPendingNudges('wren');
  check('After ack: 0 pending', afterAck.length === 0);
  store.close();
}

// --- Multiple nudges + ack-all ---
console.log('\n--- Multiple nudges ---');
{
  const store = freshStore();
  store.sendNudge('wren', 'kade', 'nudge 1');
  store.sendNudge('silas', 'kade', 'nudge 2');
  store.sendNudge('wren', 'kade', 'nudge 3');

  check('Kade has 3 pending', store.getPendingNudges('kade').length === 3);

  const acked = store.acknowledgeAllNudges('kade');
  check('Ack-all returned 3', acked === 3);
  check('Kade has 0 pending after ack-all', store.getPendingNudges('kade').length === 0);
  store.close();
}

// --- Dead Letter ---
console.log('\n--- Dead letter ---');
{
  const store = freshStore();
  const id = store.sendNudge('silas', 'wren', 'will fail');

  const r1 = store.recordDeliveryAttempt(id);
  check('Attempt 1: not dead-lettered', !r1.deadLettered);

  const r2 = store.recordDeliveryAttempt(id);
  check('Attempt 2: not dead-lettered', !r2.deadLettered);

  const r3 = store.recordDeliveryAttempt(id);
  check('Attempt 3: dead-lettered', r3.deadLettered);

  check('Pending is 0 (dead-lettered)', store.getPendingNudges('wren').length === 0);

  const dl = store.getDeadLetters();
  check('1 dead letter', dl.length === 1);
  check('Dead letter has 3 attempts', dl[0].delivery_attempts === 3);

  store.replayDeadLetter(id);
  check('After replay: 0 dead letters', store.getDeadLetters().length === 0);
  check('After replay: 1 pending', store.getPendingNudges('wren').length === 1);
  store.close();
}

// --- Chat ---
console.log('\n--- Chat ---');
{
  const store = freshStore();
  const chatId = store.startChat('silas', 'kade', 'test topic');
  check('Chat ID created', chatId.includes('silas-kade'));

  store.chatMessage(chatId, 'silas', 'hello kade');
  store.chatMessage(chatId, 'kade', 'hello silas');
  store.chatMessage(chatId, 'silas', 'how are you');

  const msgs = store.getChatMessages(chatId);
  check('3 chat messages', msgs.length === 3);
  check('First message from silas', msgs[0].from === 'silas');
  check('Second message from kade', msgs[1].from === 'kade');

  const since = store.getChatMessages(chatId, msgs[1].id);
  check('Since msg 2: 1 message', since.length === 1);
  check('Since msg 2: from silas', since[0].from === 'silas');

  store.endChat(chatId);
  store.close();
}

// --- Role State ---
console.log('\n--- Role state ---');
{
  const store = freshStore();
  store.setRoleState('silas', 'building', '1755');
  const state = store.getRoleState('silas');
  check('State is building', state?.state === 'building');
  check('Card is 1755', state?.card === '1755');

  store.setRoleState('silas', 'idle');
  const updated = store.getRoleState('silas');
  check('State updated to idle', updated?.state === 'idle');
  check('Card cleared', updated?.card === null);
  store.close();
}

// --- Query ---
console.log('\n--- Query ---');
{
  const store = freshStore();
  store.sendNudge('silas', 'wren', 'nudge A');
  store.sendNudge('kade', 'wren', 'nudge B');
  store.sendNudge('silas', 'kade', 'nudge C');

  const toWren = store.queryMessages({ to: 'wren' });
  check('Query to=wren: 2 results', toWren.length === 2);

  const fromSilas = store.queryMessages({ from: 'silas' });
  check('Query from=silas: 2 results', fromSilas.length === 2);

  const stats = store.getStats();
  check('Stats total: 3', stats.total === 3);
  check('Stats pending: 3', stats.pending === 3);
  store.close();
}

// --- Cleanup ---
if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);

console.log(`\n=== Results: ${pass} pass, ${fail} fail ===`);
process.exit(fail > 0 ? 1 : 0);
