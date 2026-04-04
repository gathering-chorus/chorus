/**
 * Socket.IO Ack Tests — #1934
 * Tests what Jeff SEES when he sends a message in the Clearing.
 */
import { readFileSync } from 'fs';
import path from 'path';

const SERVER_SRC = readFileSync(path.join(__dirname, '../src/server.ts'), 'utf-8');
const CLIENT_SRC = readFileSync(path.join(__dirname, '../public/index.html'), 'utf-8');

describe('AC1: Server jeff-message handler accepts ack callback', () => {
  test('handler signature includes ack parameter', () => {
    expect(SERVER_SRC).toMatch(/jeff-message.*ack/s);
  });
  test('server calls ack with ok field', () => {
    expect(SERVER_SRC).toMatch(/ack\?\.\(\{.*ok.*\}/s);
  });
});

describe('AC2: Client emit includes ack callback', () => {
  test('socket.emit has callback argument', () => {
    expect(CLIENT_SRC).toMatch(/emit\('jeff-message'.*function|emit\('jeff-message'.*=>/s);
  });
});

describe('AC3: Send button shows delivery state', () => {
  test('sending state exists', () => {
    expect(CLIENT_SRC).toMatch(/sending/i);
  });
  test('sent/delivered state exists', () => {
    expect(CLIENT_SRC).toMatch(/sent|delivered/i);
  });
  test('failed state exists', () => {
    expect(CLIENT_SRC).toMatch(/failed|delivery-failed/i);
  });
});

describe('AC4: Failed messages preserve input', () => {
  test('input value restored on failure', () => {
    expect(CLIENT_SRC).toMatch(/input\.value\s*=|savedText|failedText/);
  });
});
