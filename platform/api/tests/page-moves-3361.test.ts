/**
 * #3361 — chorus UI pages moved home from gathering serve from chorus-api at
 * their ADR-041 value-stream homes (building/...). Hermetic: env points at a
 * tempdir; each test drives a real request through the express `app` and asserts
 * the moved page is served by chorus. 404s before the /building static mount.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-3361-'));
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

async function hit(srv: Server, reqPath: string) {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  const port = (srv.address() as AddressInfo).port;
  const res = await fetch(`http://127.0.0.1:${port}${reqPath}`);
  return { status: res.status, body: await res.text() };
}
const close = (srv: Server) => new Promise<void>((r) => srv.close(() => r()));

describe('#3361 — moved chorus pages serve from chorus at their building/ home', () => {
  test('werk-process serves at building/products/werk', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/building/products/werk/werk-process.html');
      expect(r.status).toBe(200);
      expect(r.body).toContain('Werk Process — Card Lifecycle');
    } finally {
      await close(srv);
    }
  });

  test('nifi integration doc serves at building/products/convergence', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/building/products/convergence/nifi-chorus-integration-design.html');
      expect(r.status).toBe(200);
      expect(r.body).toContain('NiFi + Chorus Integration Design');
    } finally {
      await close(srv);
    }
  });

  test('extensionless path resolves the .html page (extensions:[html])', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/building/products/werk/werk-process');
      expect(r.status).toBe(200);
    } finally {
      await close(srv);
    }
  });
});
