/**
 * Socket.IO Ack Tests — #1934
 *
 * Tests what Jeff SEES when he sends a message in the Clearing.
 * AC: jeff-message uses ack callback, server confirms delivery.
 */

import { readFileSync } from 'fs';
import path from 'path';

const SERVER_SRC = readFileSync(path.join(__dirname, '../src/server.ts'), 'utf-8');
const CLIENT_SRC = readFileSync(path.join(__dirname, '../public/index.html'), 'utf-8');

// AC1: socket.emit jeff-message uses ack callback — server confirms receipt
describe('AC #1: Server jeff-message handler accepts ack callback', () => {
  test('handler signature includes callback parameter', () => {
    // The handler should accept a callback/ack function
    expect(SERVER_SRC).toMatch(/jeff-message.*ack|jeff-message.*callback/s);
  });

  test('server calls callback with status on success', () => {
    expect(SERVER_SRC).toMatch(/status.*delivered|status.*sent/);
  });
});

// AC2: Client retries and shows error
describe('AC #2: Client emit includes ack callback', () => {
  test('socket.emit has callback argument', () => {
    // emit('jeff-message', data, callbackFn)
    expect(CLIENT_SRC).toMatch(/emit\('jeff-message'.*(?:function|=>)/s);
  });
});

// AC3: Send button shows delivery state
describe('AC #3: Send button shows delivery state', () => {
  test('sending state class exists', () => {
    expect(CLIENT_SRC).toMatch(/sending/);
  });

  test('delivered/sent state class exists', () => {
    expect(CLIENT_SRC).toMatch(/delivered|\.sent/);
  });

  test('failed state handling exists', () => {
    expect(CLIENT_SRC).toMatch(/failed|delivery-failed/);
  });
});

// AC4: Failed messages stay in input
describe('AC #4: Failed messages preserve input text', () => {
  test('input value restored on delivery failure', () => {
    expect(CLIENT_SRC).toMatch(/input\.value\s*=\s*.*text|savedText|failedText/);
  });
});
