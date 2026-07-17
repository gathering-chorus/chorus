// @test-type: integration — in-process TestApp harness (startTestApp) + own
// owl-api stub; no live :3360, no $HOME cache (QUALITY_CACHE_PATH → tempdir).
/**
 * Quality endpoints + static page — #2099 lineage, reprojected by #3657.
 *
 * /api/chorus/quality/summary serves a projection of the tests domain
 * (owl-api V2 /tests collection), not a filesystem scan. The page at
 * /werk/quality/ (#3656 reparent) renders it.
 */

import * as http from 'http';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { startTestApp, type TestApp } from './lib/test-app';

const ROWS = [
  { name: 't1', filePath: 'platform/api/tests/a.test.ts', pyramidLayer: 'unit', hermeticity: 'hermetic', covers: 'chorus', testName: 'a1', inFile: 'sf-a' },
  { name: 't2', filePath: 'platform/api/tests/b.test.ts', pyramidLayer: 'integration', hermeticity: 'needs-stack', covers: 'chorus', testName: 'b1', inFile: 'sf-b' },
  { name: 't3', filePath: 'platform/tests/c.feature', pyramidLayer: 'bdd', hermeticity: 'hermetic', covers: 'cicd', testName: 'c1', inFile: 'sf-c' },
];

let stub: http.Server;
let tmpDir: string;

beforeAll(async () => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'quality-int-test-'));
  stub = http.createServer((req, res) => {
    const url = new URL(req.url || '/', 'http://localhost');
    if (!url.pathname.startsWith('/tests')) { res.writeHead(404).end(); return; }
    res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({
      apiVersion: 'v1', kind: 'Test', self: '/v1/tests',
      generatedFrom: { graph: 'urn:chorus:ontology', shape: 'chorus:TestShape', shapeVersion: 's1', commit: 'c1' },
      data: ROWS, links: { next: null }, count: ROWS.length,
    }));
  });
  await new Promise<void>((resolve) => stub.listen(0, resolve));
  const addr = stub.address();
  process.env.OWL_API_BASE = `http://localhost:${(addr as { port: number }).port}`;
  process.env.QUALITY_CACHE_PATH = path.join(tmpDir, 'quality-cache.json');
});

afterAll(async () => {
  await new Promise<void>((resolve) => stub.close(() => resolve()));
  delete process.env.OWL_API_BASE;
  delete process.env.QUALITY_CACHE_PATH;
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe('#3657: /api/chorus/quality/summary projects the tests domain', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('returns 200 and JSON', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 30_000);

  test('response has total, pyramid, repos, scannedAt, source', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    expect(body.total).toBe(ROWS.length);
    expect(Array.isArray(body.pyramid)).toBe(true);
    expect(Array.isArray(body.repos)).toBe(true);
    expect(typeof body.scannedAt).toBe('string');
    expect(body.source.kind).toBe('tests-domain');
    expect(body.source.generatedFrom.shape).toBe('chorus:TestShape');
  }, 30_000);

  test('repos declare the domain scope — Chorus Platform only, no fabricated Gathering rollup', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    expect(body.repos.map((r: { name: string }) => r.name)).toEqual(['Chorus Platform']);
  }, 30_000);

  test('pyramid layers use the model vocabulary with hermeticity counts', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    const keys = body.pyramid.map((l: { key: string }) => l.key);
    expect(keys).toEqual(['bdd', 'integration', 'unit']); // e2e absent from fixture
    const layer = body.pyramid[0];
    expect(layer).toHaveProperty('name');
    expect(typeof layer.count).toBe('number');
    expect(typeof layer.fileCount).toBe('number');
    expect(typeof layer.color).toBe('string');
    expect(typeof layer.hermeticCount).toBe('number');
    expect(typeof layer.needsStackCount).toBe('number');
    expect(Array.isArray(layer.files)).toBe(true);
  }, 30_000);
});

describe('#3657: /api/chorus/quality/domain/:domain filters on model covers', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /api/chorus/quality/domain/chorus returns covers-filtered layers', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/domain/chorus`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.domain).toBe('chorus');
    expect(body.total).toBe(2); // t1 + t2 cover chorus
    expect(Array.isArray(body.layers)).toBe(true);
    expect(body.layers.map((l: { key: string }) => l.key).sort()).toEqual(['integration', 'unit']);
  }, 30_000);
});

describe('#3656: /werk/quality/ static page (reparented borg→werk, #2099)', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /werk/quality/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/werk/quality/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page contains Quality heading and summary endpoint', async () => {
    const res = await fetch(`${harness.baseUrl}/werk/quality/`);
    const html = await res.text();
    expect(html).toContain('Quality');
    expect(html).toContain('/api/chorus/quality/summary');
  }, 10_000);

  test('page has pyramid container and declares the tests-domain source', async () => {
    const res = await fetch(`${harness.baseUrl}/werk/quality/`);
    const html = await res.text();
    expect(html).toContain('id="pyramid"');
    expect(html).toContain('tests domain');
  }, 10_000);

  test('old /borg/quality/ path 301s to /werk/quality/', async () => {
    for (const oldPath of ['/borg/quality', '/borg/quality/', '/borg/quality/index.html']) {
      const res = await fetch(`${harness.baseUrl}${oldPath}`, { redirect: 'manual' });
      expect(res.status).toBe(301);
      expect(res.headers.get('location')).toBe('/werk/quality/');
    }
  }, 10_000);
});
