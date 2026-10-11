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
  // #4431 reopen — 2026-10-10 20:34 the reader blocked chorus-api 3012 ms (4564 ms on
  // 10-07) re-parsing the whole 4 MB tail every refresh. A refresh reads what was added.
  test('a refresh reads only the bytes added since the last read', async () => {
    const big = Array.from({ length: 5000 }, (_, i) => line('system.heartbeat', 'silas', '2026-10-06T09:00:00Z', { n: i })).join('\n') + '\n';
    fs.appendFileSync(file, big);
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    await r.recent();
    const first = r.bytesRead;
    expect(first).toBeGreaterThan(200_000);
    const added = line('demo.verdict', 'jeff', '2026-10-06T10:00:04Z') + '\n';
    fs.appendFileSync(file, added);
    t = 2000;
    const rows = await r.recent();
    expect(r.bytesRead - first).toBe(Buffer.byteLength(added));
    expect(rows[rows.length - 1].event).toBe('demo.verdict');
    expect(rows.length).toBe(5005);
  });

  test('NEGATIVE PROOF — a reader that forgot where it was reads the whole tail again', async () => {
    fs.appendFileSync(file, Array.from({ length: 5000 }, (_, i) => line('system.heartbeat', 'silas', '2026-10-06T09:00:00Z', { n: i })).join('\n') + '\n');
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    await r.recent();
    const first = r.bytesRead;
    (r as unknown as { offset: number }).offset = -1; // the old behaviour: no memory of the last read
    fs.appendFileSync(file, line('demo.verdict', 'jeff', '2026-10-06T10:00:04Z') + '\n');
    t = 2000;
    await r.recent();
    expect(r.bytesRead - first).toBeGreaterThan(first - 1);
  });

  test('a line written in two halves is taken once, whole', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    await r.recent();
    const whole = line('demo.verdict', 'jeff', '2026-10-06T10:00:04Z');
    fs.appendFileSync(file, whole.slice(0, 20));
    t = 2000;
    expect((await r.recent()).length).toBe(4);
    fs.appendFileSync(file, whole.slice(20) + '\n');
    t = 4000;
    const rows = await r.recent();
    expect(rows.length).toBe(5);
    expect(rows[4].event).toBe('demo.verdict');
  });

  test('the window stays within tailBytes as lines arrive', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, tailBytes: 300, refreshMs: 2000, now: () => t });
    await r.recent();
    for (let i = 0; i < 20; i++) {
      fs.appendFileSync(file, line('system.heartbeat', 'kade', '2026-10-06T11:00:00Z', { n: i }) + '\n');
      t += 2000;
      await r.recent();
    }
    const rows = await r.recent();
    const bytes = rows.reduce((a, x) => a + Buffer.byteLength(JSON.stringify({ timestamp: x.timestamp, event: x.event, role: x.role })) + 1, 0);
    expect(bytes).toBeLessThanOrEqual(400);
    expect(rows[rows.length - 1].role).toBe('kade');
  });

  test('a file that shrank is read afresh from its tail', async () => {
    let t = 0;
    const r = new SpineEventsReader({ path: file, refreshMs: 2000, now: () => t });
    await r.recent();
    fs.writeFileSync(file, line('card.pulled', 'kade', '2026-10-07T10:00:00Z') + '\n');
    t = 2000;
    const rows = await r.recent();
    expect(rows.map((x) => x.role)).toEqual(['kade']);
  });
});
