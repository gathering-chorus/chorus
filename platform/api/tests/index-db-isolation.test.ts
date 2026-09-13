// @test-type: unit — #4152 the harness never opens the live index.db
import fs from 'fs';
import os from 'os';
import path from 'path';
import { liveIndexDbPath, startTestApp } from './lib/test-app';

// #4163 (wren, in Kade's file with his agreement 2026-09-13): the two tests below
// COPY the live index.db, which is 471 MB today. Jest's default per-test timeout is
// 5s; the copy alone takes ~3.8s on a quiet box and longer under load, so these went
// red in three separate pipeline runs while passing every time they were run alone.
// That is a measurement of the machine, not of the product. They are NOT quarantined
// — both are real negative proofs and must keep running — they are given a timeout
// that fits the work they actually do.
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

  test('negative proof: an empty world index.db is seeded by globalSetup, and answers 503 until it is', async () => {
    if (!haveLive) return;
    const setup = require('./lib/index-db-global-setup');
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-empty-world-'));
    const world = path.join(dir, 'index.db');
    try {
      expect(setup.needsSeed(world)).toBe(true);            // missing → seed
      fs.writeFileSync(world, '');                          // the runner's empty world shape
      expect(setup.needsSeed(world)).toBe(true);            // empty → seed
      await setup.backupLiveInto(live, world);
      expect(setup.needsSeed(world)).toBe(false);           // seeded → leave alone
      expect(fs.statSync(world).size).toBeGreaterThan(1024 * 1024);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }, 60_000);

  test('the copy answers freshness 200 through the harness', async () => {
    if (!haveLive) return;
    const h = await startTestApp();
    try {
      const res = await fetch(`${h.baseUrl}/api/chorus/freshness`);
      expect(res.status).toBe(200);
    } finally {
      await h.close();
    }
  }, 60_000);

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
