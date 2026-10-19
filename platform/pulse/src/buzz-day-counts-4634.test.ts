// @test-type: unit — a temp sqlite store, no relay
// @domain: messages
/**
 * #4634 — the messages sensor's Buzz reader. It reads only nudges and Jeff's
 * inputs from pulse's store, and it reports UNMEASURED (exit 3, nothing on
 * stdout) whenever it cannot count, never a day of zeros.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import Database from 'better-sqlite3';
import { main, readRows } from './buzz-day-counts';

function store(rows: [string, string][]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-days-4634-'));
  const file = path.join(dir, 'messages.db');
  const d = new Database(file);
  d.exec(`CREATE TABLE messages (created_at TEXT, type TEXT, "from" TEXT, "to" TEXT, content TEXT)`);
  const ins = d.prepare(`INSERT INTO messages VALUES (?, ?, 'kade', 'silas', 'hi')`);
  for (const [at, type] of rows) ins.run(at, type);
  d.close();
  return file;
}

describe('#4634 the Buzz reader counts only what can be counted', () => {
  let out: jest.SpyInstance;
  let err: jest.SpyInstance;
  beforeEach(() => {
    out = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);
    err = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
  });
  afterEach(() => { out.mockRestore(); err.mockRestore(); });

  test('reads nudges and jeff-inputs, and leaves every other row type out', () => {
    const db = store([['2026-10-17 14:00:00', 'nudge'], ['2026-10-17 15:00:00', 'jeff-input'], ['2026-10-17 16:00:00', 'presence']]);
    expect(readRows(db).map((r) => r.type).sort()).toEqual(['jeff-input', 'nudge']);
  });

  test('no relay configured is UNMEASURED: exit 3 and no day printed', async () => {
    const db = store([['2026-10-17 14:00:00', 'nudge']]);
    expect(await main(['node', 'x', '--db', db], {})).toBe(3);
    expect(out).not.toHaveBeenCalled();
    expect(String(err.mock.calls[0][0])).toContain('UNMEASURED');
  });

  test('an empty store is UNMEASURED, not a day of zeros', async () => {
    const db = store([]);
    expect(await main(['node', 'x', '--db', db], { PULSE_BUZZ_RELAY_URL: 'ws://127.0.0.1:1' })).toBe(3);
    expect(out).not.toHaveBeenCalled();
  });

  test('a relay that cannot be reached is UNMEASURED', async () => {
    const db = store([['2026-10-17 14:00:00', 'nudge']]);
    expect(await main(['node', 'x', '--db', db], { PULSE_BUZZ_RELAY_URL: 'ws://127.0.0.1:1' })).toBe(3);
    expect(out).not.toHaveBeenCalled();
  });
});
