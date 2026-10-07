// @test-type: unit — a throwaway SQLite file in the test's temp dir; no service, no real index.
// #3060 — computeFreshness is the recompute the freshness worker runs: open the
// index read-only, run fetchFreshness, close it.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import Database from 'better-sqlite3';
import { computeFreshness } from '../src/freshness-compute';

describe('computeFreshness (#3060)', () => {
  let dir: string;
  beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'freshness-3060-')); });
  afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

  it('answers 503 when the index file does not exist', () => {
    const r = computeFreshness(path.join(dir, 'missing.db'), path.join(dir, 'chorus.log'));
    expect(r.status).toBe(503);
    expect(r.body).toEqual({ error: 'Index database not found' });
  });

  it('answers 200 with per-source rows from a real index file', () => {
    const dbPath = path.join(dir, 'index.db');
    const db = new Database(dbPath);
    db.exec(`CREATE TABLE watermarks (source TEXT, last_indexed TEXT);
             CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT);
             INSERT INTO watermarks VALUES ('clearing', '2026-10-07T15:00:00Z');`);
    db.close();
    const r = computeFreshness(dbPath, path.join(dir, 'chorus.log'));
    expect(r.status).toBe(200);
    expect(JSON.stringify(r.body)).toContain('clearing');
  });
});
