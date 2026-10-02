/**
 * @test-type: integration:api
 *
 * Domain API consolidation tests — #2060
 *
 * One endpoint per facet under /api/chorus/domain/:name/.
 * Same shape whether Jeff sees it on the domain page or a role
 * gets it during /pull. AX = UX.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2060: consolidated domain API', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  // --- /code ---

  test('GET /api/chorus/domain/:name/code returns code files', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.files)).toBe(true);
    expect(body._meta).toBeDefined();
    expect(body._meta.source_count).toBeDefined();
  });

  test('/code does not include test files — tests have own endpoint', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/code`);
    const body = await res.json();
    const testFiles = body.data.files.filter(function(f) {
      return /\/(tests?|__tests__)\//.test(f.path) || /\.(test|spec)\./.test(f.path);
    });
    expect(testFiles.length).toBe(0);
  });

  // #4416 — the '-domain' suffix case is gone (#4353).


  // --- /tests ---

  test('GET /api/chorus/domain/:name/tests returns test coverage', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/tests`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.tests)).toBe(true);
    expect(body.data.byType).toBeDefined();
    expect(body._meta.count).toBeDefined();
  });

  // --- /alerts ---

  test('GET /api/chorus/domain/:name/alerts returns alert rules', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/alerts`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.alerts)).toBe(true);
    expect(body._meta.count).toBeDefined();
  });

  // --- /logs --- retired in #4084 (Logs fold reads the graph); no facet route (#4143)

  // --- /services ---

  test('GET /api/chorus/domain/:name/services returns endpoints', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/services`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.endpoints)).toBe(true);
    expect(body._meta.count).toBeDefined();
  });

  // --- Consistent envelope ---

  test('all four facet endpoints use identical envelope shape', async () => {
    const facets = ['code', 'tests', 'alerts', 'services'];
    const responses = await Promise.all(
      facets.map(f => fetch(`${harness.baseUrl}/api/chorus/domain/seeds/${f}`).then(r => r.json()))
    );
    for (const body of responses) {
      expect(body).toHaveProperty('_meta');
      expect(body).toHaveProperty('data');
      expect(body._meta).toHaveProperty('source', 'athena');
      expect(typeof body._meta.duration_ms).toBe('number');
    }
  });

  // --- Blast radius consumer ---

  test('blast radius can use /code endpoint — files have path strings', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/code`);
    const body = await res.json();
    const filePaths = body.data.files.map(function(f) { return f.path; });
    // #3559: dropped "length > 0" — blast radius consuming /code needs path
    // STRINGS (the contract), not a non-empty census of the seeds domain (data,
    // invariant #4). Assert every returned path is a string.
    expect(Array.isArray(filePaths)).toBe(true);
    expect(filePaths.every(function(p) { return typeof p === 'string'; })).toBe(true);
  });

  // --- Empty result for unknown domain ---

  test('returns empty data for unknown domain, not 500', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/nonexistent-xyz/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.files).toEqual([]);
  });
});
