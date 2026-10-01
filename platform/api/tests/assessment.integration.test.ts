// @test-type: integration:api — in-process TestApp harness
/**
 * Borg Assessment endpoint + static page — #2099
 *
 * Per-page migration: Borg Assessment shows capability maturity across
 * 5 value-stream buckets × ~25 domains with codebase/harvest/graph status.
 * Topology data is proxied from Gathering (lives in its RDF store there).
 * Churn panel degrades gracefully until admin-auth path is sorted.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099: /api/chorus/codebase/topology proxy', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('returns 200 and JSON when Gathering is up', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/codebase/topology`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  });

  test('response has summary, spokes, domains', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/codebase/topology`);
    const body = await res.json();
    expect(body.summary).toBeDefined();
    expect(body.spokes).toBeDefined();
    expect(body.domains).toBeDefined();
  });
});

describe('#2099: /borg/assessment/ static page', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /borg/assessment/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/assessment/`);
    expect(res.status).toBe(200);
  });

  test('page has Borg Assessment heading and topology endpoint', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/assessment/`);
    const html = await res.text();
    expect(html).toContain('Borg Assessment');
    expect(html).toContain('/api/chorus/codebase/topology');
  });

  test('page has grid body + value-stream sections', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/assessment/`);
    const html = await res.text();
    expect(html).toContain('id="grid-body"');
    expect(html).toContain('Sowing');
    expect(html).toContain('Reflecting');
  });
});
