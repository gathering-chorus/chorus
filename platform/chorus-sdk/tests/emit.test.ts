import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { emit } from '../src/emit';

describe('emit', () => {
  const tmpFile = path.join(os.tmpdir(), `chorus-sdk-test-${Date.now()}.log`);

  afterAll(() => {
    try { fs.unlinkSync(tmpFile); } catch {}
  });

  it('writes a JSON line to the log file', () => {
    const event = emit('test.event', 'silas', { card: '972' }, { logFile: tmpFile });

    expect(event.event).toBe('test.event');
    expect(event.role).toBe('silas');
    expect(event.card).toBe('972');
    expect(event.appName).toBe('chorus-sdk');
    expect(event.timestamp).toBeTruthy();

    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    expect(lines).toHaveLength(1);
    const parsed = JSON.parse(lines[0]);
    expect(parsed.event).toBe('test.event');
    expect(parsed.role).toBe('silas');
  });

  it('appends multiple events', () => {
    emit('event.one', 'wren', {}, { logFile: tmpFile });
    emit('event.two', 'kade', {}, { logFile: tmpFile });

    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    expect(lines.length).toBeGreaterThanOrEqual(3);
  });

  it('uses custom appName and component', () => {
    const event = emit('custom.event', 'silas', {}, {
      logFile: tmpFile,
      appName: 'board-client',
      component: 'cli',
    });

    expect(event.appName).toBe('board-client');
    expect(event.component).toBe('cli');
  });

  it('is backward compatible with board-client format', () => {
    const event = emit('card.accepted', 'silas', { card_id: '177' }, { logFile: tmpFile });
    const lines = fs.readFileSync(tmpFile, 'utf-8').trim().split('\n');
    const last = JSON.parse(lines[lines.length - 1]);

    expect(last).toHaveProperty('timestamp');
    expect(last).toHaveProperty('level', 'info');
    expect(last).toHaveProperty('appName');
    expect(last).toHaveProperty('component');
    expect(last).toHaveProperty('event', 'card.accepted');
    expect(last).toHaveProperty('role', 'silas');
    expect(last).toHaveProperty('card_id', '177');
  });
});
