// @test-type: unit — the events domain's one spine reader, against a temp log file
// @card: #4431
// @owner: wren
/**
 * #4431 — one reader of the spine, shared by every consumer.
 * The freeze it ends: /context/roles read the last 4 MB of chorus.log three
 * times per call, synchronously, ~1700 calls an hour.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { SpineEventsReader, filterSpineEvents, parseSpineLine } from '../src/lib/spine-events';

function line(event: string, role: string, timestamp: string, extra: Record<string, unknown> = {}): string {
  return JSON.stringify({ timestamp, event, role, ...extra });
}

describe('#4431 SpineEventsReader', () => {
  let dir: string;
  let file: string;
  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'spine-4431-'));
    file = path.join(dir, 'chorus.log');
    fs.writeFileSync(file, [
      line('card.pulled', 'wren', '2026-10-06T10:00:00Z', { card_id: 4431 }),
      line('system.heartbeat', 'silas', '2026-10-06T10:00:01Z'),
      line('demo.presented', 'kade', '2026-10-06T10:00:02Z'),
      line('card.pulled', 'silas', '2026-10-06T10:00:03Z'),
    ].join('\n') + '\n');
  });
  afterEach(() => fs.rmSync(dir, { recursive: true, force: true }));

  test('callers inside one refresh window share ONE read of the file', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    await Promise.all([r.recent(), r.recent(), r.recent()]); // concurrent, like 3 roles in one /context/roles call
    t = 1999;
    await r.query({ role: 'wren' });
    expect(r.reads).toBe(1);
  });

  test('NEGATIVE PROOF — with no window, every call reads the file (the old per-role behaviour)', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 0, now: () => t++ });
    await r.recent();
    await r.recent();
    expect(r.reads).toBe(2);
  });

  test('after the window, a line written since is seen', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    expect((await r.recent()).length).toBe(4);
    fs.appendFileSync(file, line('demo.verdict', 'jeff', '2026-10-06T10:00:04Z') + '\n');
    t = 2000;
    const rows = await r.recent();
    expect(rows.length).toBe(5);
    expect(rows[4].event).toBe('demo.verdict');
  });

  test('filters by role, type, since and limit', async () => {
    const r = new SpineEventsReader({ path: file });
    const rows = await r.recent();
    expect(filterSpineEvents(rows, { role: 'silas' }).map((x) => x.event)).toEqual(['system.heartbeat', 'card.pulled']);
    expect(filterSpineEvents(rows, { types: ['card.pulled'] }).map((x) => x.role)).toEqual(['wren', 'silas']);
    expect(filterSpineEvents(rows, { sinceMs: Date.parse('2026-10-06T10:00:02Z') }).length).toBe(2);
    expect(filterSpineEvents(rows, { limit: 1 })[0].event).toBe('card.pulled');
    expect(filterSpineEvents(rows, { limit: 1 })[0].role).toBe('silas'); // the most recent one
  });

  test('a spine that cannot be read rejects; it never reads as empty', async () => {
    const r = new SpineEventsReader({ path: path.join(dir, 'missing.log') });
    await expect(r.recent()).rejects.toThrow();
  });

  test('a tail cut mid-line drops the partial line, keeps the rest', async () => {
    // one full last line (~72 bytes) plus part of the one before
    const r = new SpineEventsReader({ path: file, tailBytes: 100 });
    const rows = await r.recent();
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.every((x) => typeof x.event === 'string' && Number.isFinite(x.ts))).toBe(true);
    expect(parseSpineLine('"role":"wren","event":"card.pulled"}')).toBeNull();
  });
});
