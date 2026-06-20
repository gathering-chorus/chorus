// @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
/**
 * #3408 — re-live the moved /werk cockpit. Its client JS fetches /api/werk/schema
 * (+ /api/werk/activity, /api/loom-metrics) which 404'd on chorus-api after the
 * #3361 page move. These endpoints feed the cockpit from chorus-api's own data so
 * /werk hydrates live instead of showing a frozen-empty shell. Hermetic: drive a
 * real request through the express app.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-3408-'));
process.env.CHORUS_ROOT = TMP;
process.env.DB_PATH = path.join(TMP, 'test.db');
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.CLAUDE_STATS_CACHE = path.join(TMP, 'stats.json');
process.env.CLEARING_TRANSCRIPTS_DIR = path.join(TMP, 'transcripts');
process.env.QUALITY_CACHE_PATH = path.join(TMP, 'quality.json');
process.env.POSTURE_BASE = path.join(TMP, 'posture');
process.env.SESSIONS_DIR = path.join(TMP, 'sessions');

import app from '../src/server';
import type { AddressInfo } from 'net';
import type { Server } from 'http';

async function get(srv: Server, reqPath: string) {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  const port = (srv.address() as AddressInfo).port;
  const res = await fetch(`http://127.0.0.1:${port}${reqPath}`);
  const text = await res.text();
  return { status: res.status, text };
}
const close = (srv: Server) => new Promise<void>((r) => srv.close(() => r()));

describe('#3408 — /werk cockpit data endpoints serve from chorus-api', () => {
  test('/api/werk/schema returns the spine-events schema JSON', async () => {
    const srv = app.listen(0);
    try {
      const r = await get(srv, '/api/werk/schema');
      expect(r.status).toBe(200);
      const json = JSON.parse(r.text);
      expect(typeof json).toBe('object');
      expect(Object.keys(json).length).toBeGreaterThan(0);
    } finally { await close(srv); }
  });

  test('/api/werk/activity returns {entries:[]} contract shape (the cockpit feed)', async () => {
    const srv = app.listen(0);
    try {
      const r = await get(srv, '/api/werk/activity?hours=168');
      expect(r.status).toBe(200);
      const json = JSON.parse(r.text);
      expect(Array.isArray(json.entries)).toBe(true);
    } finally { await close(srv); }
  });

  test('/api/loom-metrics returns the fitness-panel shape (board/weekly_throughput/reject_stats/operations)', async () => {
    const srv = app.listen(0);
    try {
      const r = await get(srv, '/api/loom-metrics');
      expect(r.status).toBe(200);
      const m = JSON.parse(r.text);
      expect(typeof m.board).toBe('object');
      expect(typeof m.weekly_throughput).toBe('object');
      expect(typeof m.reject_stats).toBe('object');
      expect(typeof m.operations.deploys).toBe('number');
    } finally { await close(srv); }
  });
});
