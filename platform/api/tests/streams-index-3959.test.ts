// @test-type: integration
//
// #3959 — the spine index. Every test brings its own world: a temp spine file
// and an in-memory-adjacent temp db, never ~/.chorus and never the live store.

// #4004 — platform/api runs JEST, not vitest; this import made the suite fail
// to RUN ("Cannot find module 'vitest'"), which the coverage lane reported as
// "coverage run errored, no clean measurement" — a whole package unmeasured
// because one file imported the wrong runner. Jest provides these globals.
import { mkdtempSync, rmSync, writeFileSync, appendFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  openStreamsDb,
  indexSpine,
  attributionRate,
  actionsFor,
} from '../src/streams-index';

let dir: string;
let spine: string;
let dbPath: string;

const line = (o: Record<string, unknown>) =>
  JSON.stringify({ appName: 'chorus-events', level: 'info', ...o }) + '\n';

const beat = (role: string | null, ts: string, tool = 'Bash') =>
  line({
    timestamp: ts,
    event: 'agent.activity',
    ...(role === null ? {} : { role }),
    phase: 'running',
    tool,
    trace_id: 'trace-1',
  });

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'streams-3959-'));
  spine = join(dir, 'chorus.log');
  dbPath = join(dir, 'streams.db');
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

describe('#3959 — the spine becomes queryable', () => {
  it('indexes spine lines and answers "what did <role> do between t1 and t2"', () => {
    writeFileSync(
      spine,
      beat('wren', '2026-08-21T18:00:00.000-0400') +
        beat('wren', '2026-08-21T18:05:00.000-0400', 'Edit') +
        beat('silas', '2026-08-21T18:06:00.000-0400'),
    );
    const db = openStreamsDb(dbPath);
    const res = indexSpine(db, spine);
    expect(res.inserted).toBe(3);

    const acts = actionsFor(
      db,
      'wren',
      '2026-08-21T18:00:00.000-0400',
      '2026-08-21T18:05:00.000-0400',
    );
    expect(acts).toHaveLength(2);
    expect(acts[0].tool).toBe('Bash');
    expect(acts[1].tool).toBe('Edit');
    db.close();
  });

  it('resumes at the stored offset — a re-run costs only the new bytes', () => {
    writeFileSync(spine, beat('wren', '2026-08-21T18:00:00.000-0400'));
    const db = openStreamsDb(dbPath);
    expect(indexSpine(db, spine).inserted).toBe(1);
    // nothing new
    expect(indexSpine(db, spine).inserted).toBe(0);
    appendFileSync(spine, beat('kade', '2026-08-21T18:01:00.000-0400'));
    expect(indexSpine(db, spine).inserted).toBe(1);
    expect(attributionRate(db).total).toBe(2);
    db.close();
  });

  // NEGATIVE PROOF — a partial trailing line must not be indexed OR counted as
  // corruption. A growing file always has one; treating it as malformed would
  // make a healthy tail look broken.
  it('a partial trailing line is left for the next run, not counted malformed', () => {
    writeFileSync(spine, beat('wren', '2026-08-21T18:00:00.000-0400') + '{"partial":');
    const db = openStreamsDb(dbPath);
    const first = indexSpine(db, spine);
    expect(first.inserted).toBe(1);
    expect(first.malformed).toBe(0);

    // complete the line; the next run picks it up whole
    writeFileSync(
      spine,
      beat('wren', '2026-08-21T18:00:00.000-0400') +
        beat('wren', '2026-08-21T18:02:00.000-0400'),
    );
    const second = indexSpine(db, spine);
    expect(second.inserted).toBe(1);
    expect(second.malformed).toBe(0);
    db.close();
  });

  // NEGATIVE PROOF — the rate must not report 100% over an empty window. That
  // would make a DEAD indexer indistinguishable from a perfectly attributed one,
  // which is the defect class this whole card exists to kill.
  it('an empty window reports 0, never 100%', () => {
    const db = openStreamsDb(dbPath);
    const empty = attributionRate(db);
    expect(empty.total).toBe(0);
    expect(empty.rate).toBe(0);
    expect(empty.rate).not.toBe(1);
    db.close();
  });

  // NEGATIVE PROOF — unattributed rows are COUNTED, and they drag the rate down.
  // Silently dropping them at index time would hide the 26,000/day all over again.
  it('unattributed events are indexed and counted, and lower the rate', () => {
    writeFileSync(
      spine,
      beat('wren', '2026-08-21T18:00:00.000-0400') +
        beat('unknown', '2026-08-21T18:00:01.000-0400') +
        beat('unattributed', '2026-08-21T18:00:02.000-0400') +
        beat(null, '2026-08-21T18:00:03.000-0400'),
    );
    const db = openStreamsDb(dbPath);
    const res = indexSpine(db, spine);
    expect(res.inserted).toBe(4);
    expect(res.unattributed).toBe(3);

    const rate = attributionRate(db);
    expect(rate.total).toBe(4);
    expect(rate.attributed).toBe(1);
    expect(rate.rate).toBeCloseTo(0.25, 5);
    expect(rate.byRole['unknown']).toBe(1);
    expect(rate.byRole['<missing>']).toBe(1);
    db.close();
  });

  // NEGATIVE PROOF — a spine that shrank was rotated or replaced. Resuming at
  // the old offset would index whatever now happens to sit there.
  it('a shrunken spine restarts from zero rather than resuming mid-file', () => {
    writeFileSync(
      spine,
      beat('wren', '2026-08-21T18:00:00.000-0400') +
        beat('wren', '2026-08-21T18:01:00.000-0400') +
        beat('wren', '2026-08-21T18:02:00.000-0400'),
    );
    const db = openStreamsDb(dbPath);
    expect(indexSpine(db, spine).inserted).toBe(3);

    // replaced with a shorter file
    writeFileSync(spine, beat('kade', '2026-08-21T19:00:00.000-0400'));
    const after = indexSpine(db, spine);
    expect(after.inserted).toBe(1);
    expect(attributionRate(db).byRole['kade']).toBe(1);
    db.close();
  });

  it('malformed JSON is counted, not silently skipped', () => {
    writeFileSync(spine, 'not json at all\n' + beat('wren', '2026-08-21T18:00:00.000-0400'));
    const db = openStreamsDb(dbPath);
    const res = indexSpine(db, spine);
    expect(res.inserted).toBe(1);
    expect(res.malformed).toBe(1);
    db.close();
  });
});
