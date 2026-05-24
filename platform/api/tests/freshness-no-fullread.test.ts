// #3060 - fetchFreshness must derive spine drift from an injected line COUNT
// (streaming), not by reading the whole 170MB spine log into a string + split().
// This exercises the real behavior: the injected countLines value flows into the
// spine source's `unindexed` drift. If someone reverts to a whole-file read, the
// countLines dep is ignored and this test fails.

import Database from 'better-sqlite3';
import { fetchFreshness } from '../src/handlers/chorus-freshness';

function seedDb(spineMessages: number): Database.Database {
  const db = new Database(':memory:');
  db.exec('CREATE TABLE watermarks (source TEXT, last_indexed TEXT)');
  db.exec('CREATE TABLE messages (source TEXT)');
  const recent = new Date().toISOString();
  db.prepare('INSERT INTO watermarks (source, last_indexed) VALUES (?, ?)').run('spine', recent);
  const ins = db.prepare("INSERT INTO messages (source) VALUES ('spine')");
  for (let i = 0; i < spineMessages; i++) ins.run();
  return db;
}

describe('fetchFreshness spine drift (#3060 - streaming count, no whole-file read)', () => {
  it('uses the injected countLines value as spine on-disk count for drift', () => {
    const db = seedDb(900); // 900 spine rows indexed
    let countLinesCalls = 0;
    const r = fetchFreshness({
      db,
      exists: () => true,
      countLines: () => { countLinesCalls++; return 1100; }, // 1100 lines on disk
      spineLogPath: '/does/not/matter',
      now: () => Date.now(),
    });
    db.close();

    expect(countLinesCalls).toBe(1); // the count fn was actually used (not a file read)
    const spine = (r.body as any).sources.find((s: any) => s.source === 'spine');
    expect(spine).toBeDefined();
    expect(spine.unindexed).toBe(200); // 1100 on disk - 900 indexed
    expect(spine.level).toBe('critical'); // 100..999 unindexed = critical
  });

  it('counts zero on-disk when the spine log is absent (exists=false), no read attempted', () => {
    const db = seedDb(50);
    let countLinesCalls = 0;
    const r = fetchFreshness({
      db,
      exists: () => false, // log missing
      countLines: () => { countLinesCalls++; return 999; },
      spineLogPath: '/does/not/matter',
      now: () => Date.now(),
    });
    db.close();

    expect(countLinesCalls).toBe(0); // never tries to count a missing file
    const spine = (r.body as any).sources.find((s: any) => s.source === 'spine');
    expect(spine.unindexed).toBe(0); // onDisk 0 -> drift clamps to 0 -> fresh
    expect(spine.level).toBe('fresh');
  });
});
