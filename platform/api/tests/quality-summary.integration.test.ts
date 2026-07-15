// @test-type: integration — in-process TestApp harness (startTestApp); no live services.
/**
 * Quality Service endpoint + static page — #2099
 *
 * Per-page migration: Quality Service (test pyramid) from Gathering EJS
 * to Chorus. Scans both Gathering and Chorus repos for test files, counts
 * cases per layer, classifies each by kind (api/ui/other) and domain.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099: /api/chorus/quality/summary', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('returns 200 and JSON', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 30_000);

  test('response has total, pyramid, repos, scannedAt', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    expect(typeof body.total).toBe('number');
    expect(Array.isArray(body.pyramid)).toBe(true);
    expect(Array.isArray(body.repos)).toBe(true);
    expect(typeof body.scannedAt).toBe('string');
  }, 30_000);

  test('total is positive (real test files exist)', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    expect(body.total).toBeGreaterThan(0);
  }, 30_000);

  test('repos include Gathering App and Chorus Platform', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    const names = body.repos.map(r => r.name);
    expect(names).toContain('Gathering App');
    expect(names).toContain('Chorus Platform');
  }, 30_000);

  test('pyramid layers have name, key, count, fileCount, color, files', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/summary`);
    const body = await res.json();
    const layer = body.pyramid[0];
    expect(layer).toHaveProperty('name');
    expect(layer).toHaveProperty('key');
    expect(typeof layer.count).toBe('number');
    expect(typeof layer.fileCount).toBe('number');
    expect(typeof layer.color).toBe('string');
    expect(Array.isArray(layer.files)).toBe(true);
  }, 30_000);
});

describe('#2099: /api/chorus/quality/domain/:domain', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/chorus/quality/domain/chorus returns domain filter', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/quality/domain/chorus`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.domain).toBe('chorus');
    expect(typeof body.total).toBe('number');
    expect(Array.isArray(body.layers)).toBe(true);
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

  test('page has pyramid container', async () => {
    const res = await fetch(`${harness.baseUrl}/werk/quality/`);
    const html = await res.text();
    expect(html).toContain('id="pyramid"');
  }, 10_000);

  test('old /borg/quality/ path 301s to /werk/quality/', async () => {
    for (const oldPath of ['/borg/quality', '/borg/quality/', '/borg/quality/index.html']) {
      const res = await fetch(`${harness.baseUrl}${oldPath}`, { redirect: 'manual' });
      expect(res.status).toBe(301);
      expect(res.headers.get('location')).toBe('/werk/quality/');
    }
  }, 10_000);
});
