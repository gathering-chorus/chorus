// @test-type: unit — #4152 the harness never opens the live index.db
import fs from 'fs';
import os from 'os';
import path from 'path';
import { liveIndexDbPath, startTestApp } from './lib/test-app';

describe('#4152 api test harness brings its own index.db', () => {
  const live = liveIndexDbPath();
  const haveLive = fs.existsSync(live);

  test('CHORUS_DB_PATH points at a seeded copy, not the live file', () => {
    if (!haveLive) return; // no live index on this box: nothing to isolate
    const p = process.env.CHORUS_DB_PATH;
    expect(p).toBeDefined();
    expect(path.resolve(p!)).not.toBe(path.resolve(live));
    expect(fs.existsSync(p!)).toBe(true);
    // a seeded copy carries rows; the runner's empty world schema is ~36 KB
    expect(fs.statSync(p!).size).toBeGreaterThan(1024 * 1024);
  });

  // #4163 (wren, at Kade's request 2026-09-13). This proof used to call
  // backupLiveInto(live, world) — a real copy of ~/.chorus/index.db, 471 MB
  // today and growing. It went red in three pipeline runs at the 5s default and
  // again at 60s, while passing alone in 3.8s: it was measuring disk throughput,
  // not the harness contract. Any timeout large enough would expire as the file
  // grows. Split into the two things it actually proved, neither of which needs
  // half a gigabyte. The real seeding of the real index still happens in
  // globalSetup, where it belongs and is not an assertion.

  test('negative proof: needsSeed says yes for missing and empty, no for a seeded file', () => {
    const setup = require('./lib/index-db-global-setup');
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-empty-world-'));
    const world = path.join(dir, 'index.db');
    try {
      expect(setup.needsSeed(world)).toBe(true);            // missing → seed
      fs.writeFileSync(world, '');                          // the runner's empty world shape
      expect(setup.needsSeed(world)).toBe(true);            // empty → seed
      // a file over the 1 MB floor reads as seeded. The floor is the whole rule:
      // an empty schema is ~36 KB, a seeded index is hundreds of MB.
      fs.writeFileSync(world, Buffer.alloc(1024 * 1024 + 1));
      expect(setup.needsSeed(world)).toBe(false);           // seeded → leave alone
      // and the boundary is where it claims to be, not merely somewhere below
      fs.writeFileSync(world, Buffer.alloc(1024 * 1024 - 1));
      expect(setup.needsSeed(world)).toBe(true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  test('backupLiveInto copies a sqlite database faithfully, rows and all', async () => {
    const setup = require('./lib/index-db-global-setup');
    const Database = require('better-sqlite3');
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-backup-'));
    const src = path.join(dir, 'src.db');
    const dest = path.join(dir, 'copy.db');
    try {
      const db = new Database(src);
      // WAL on purpose: the live index runs in WAL mode, and backupLiveInto's
      // whole claim is that an online backup carries the WAL. better-sqlite3
      // opens in the default journal mode, so a fixture left at the default
      // would let that claim pass untested — Kade caught this on review.
      db.pragma('journal_mode = WAL');
      db.exec('CREATE TABLE t (k TEXT PRIMARY KEY, v TEXT)');
      db.prepare('INSERT INTO t VALUES (?, ?)').run('seeded', 'yes');
      // written AFTER the checkpoint boundary, so this row lives in the -wal
      // file rather than the main db when the backup runs
      db.prepare('INSERT INTO t VALUES (?, ?)').run('in-wal', 'yes');
      expect(fs.existsSync(src + '-wal')).toBe(true);

      await setup.backupLiveInto(src, dest);
      db.close();

      const copy = new Database(dest, { readonly: true, fileMustExist: true });
      try {
        expect(copy.prepare('SELECT v FROM t WHERE k = ?').get('seeded')).toEqual({ v: 'yes' });
        // the WAL-resident row is the one a naive file copy would lose
        expect(copy.prepare('SELECT v FROM t WHERE k = ?').get('in-wal')).toEqual({ v: 'yes' });
      } finally {
        copy.close();
      }
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  test('the copy answers freshness 200 through the harness', async () => {
    if (!haveLive) return;
    const h = await startTestApp();
    try {
      const res = await fetch(`${h.baseUrl}/api/chorus/freshness`);
      expect(res.status).toBe(200);
    } finally {
      await h.close();
    }
  });

  test('negative proof: pointed at the live file, the harness refuses to start', async () => {
    if (!haveLive) return;
    const saved = process.env.CHORUS_DB_PATH;
    process.env.CHORUS_DB_PATH = live;
    try {
      await expect(startTestApp()).rejects.toThrow(/refuses the live index.db/);
    } finally {
      process.env.CHORUS_DB_PATH = saved;
    }
  });

  test('negative proof: unset, the harness refuses to start', async () => {
    if (!haveLive) return;
    const saved = process.env.CHORUS_DB_PATH;
    delete process.env.CHORUS_DB_PATH;
    try {
      await expect(startTestApp()).rejects.toThrow(/refuses the live index.db/);
    } finally {
      process.env.CHORUS_DB_PATH = saved;
    }
  });
});
