// @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
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

async function hit(srv: Server, reqPath: string, redirect: RequestRedirect = 'follow') {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  const port = (srv.address() as AddressInfo).port;
  const res = await fetch(`http://127.0.0.1:${port}${reqPath}`, { redirect });
  return { status: res.status, body: await res.text(), location: res.headers.get('location') };
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

describe('#3361 — server-rendered chorus pages serve from chorus-api (EJS moved home)', () => {
  // Move home now; client-side data may be broken until ported (Jeff 2026-06-13).
  // These assert the page SHELL renders from chorus, not that live data is wired.
  test('/chorus renders the chorus-system page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/chorus');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/chorus-model-data renders from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/chorus-model-data');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/borg-assessment renders from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/borg-assessment');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/harvesting/icd renders the ICD page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/harvesting/icd');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/harvesting/convergence renders the ICD page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/harvesting/convergence');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/werk renders from chorus-api with empty-workflow fallback', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/werk');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/harvest-manifests renders from chorus-api with empty fallback', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/harvest-manifests');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/chorus/system redirects to /chorus', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/chorus/system', 'manual');
      expect(r.status).toBe(301);
      expect(r.location).toBe('/chorus');
    } finally { await close(srv); }
  });
  test('/harvesting/mapper redirects to /harvesting/icd', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/harvesting/mapper', 'manual');
      expect(r.status).toBe(301);
      expect(r.location).toBe('/harvesting/icd');
    } finally { await close(srv); }
  });
});

describe('#3361 — service-backed chorus pages serve a shell from chorus-api (data wiring follow-on)', () => {
  // loom/flow/model-data are home in chorus; their live data (TeamService/
  // SparqlService) is the prioritized follow-on. The route must always serve a
  // valid page (real view if it renders, shell fallback otherwise) — never 500.
  test('/loom serves a page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/loom');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/loom/:role serves a page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/loom/kade');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/flow serves a page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/flow');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/model-data serves a page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/model-data');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
  test('/ontology-views/:domain serves a page from chorus-api', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/ontology-views/photos');
      expect(r.status).toBe(200);
      expect(r.body.toLowerCase()).toContain('<html');
    } finally { await close(srv); }
  });
});
