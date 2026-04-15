/**
 * Domain API consolidation tests — #2060
 *
 * One endpoint per facet under /api/chorus/domain/:name/.
 * Same shape whether Jeff sees it on the domain page or a role
 * gets it during /pull. AX = UX.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2060: consolidated domain API', () => {

  // --- /code ---

  test('GET /api/chorus/domain/:name/code returns code files', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data.subdomain).toBe('seeds-domain');
    expect(Array.isArray(body.data.files)).toBe(true);
    expect(body._meta).toBeDefined();
    expect(body._meta.source_count).toBeDefined();
  }, 10_000);

  test('/code does not include test files — tests have own endpoint', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/code`);
    const body = await res.json();
    const testFiles = body.data.files.filter(function(f) {
      return /\/(tests?|__tests__)\//.test(f.path) || /\.(test|spec)\./.test(f.path);
    });
    expect(testFiles.length).toBe(0);
  }, 10_000);

  test('/code accepts domain name with or without suffix', async () => {
    const r1 = await fetch(`${API}/api/chorus/domain/seeds/code`);
    const r2 = await fetch(`${API}/api/chorus/domain/seeds-domain/code`);
    expect(r1.status).toBe(200);
    expect(r2.status).toBe(200);
    const b1 = await r1.json();
    const b2 = await r2.json();
    expect(b1.data.files.length).toBe(b2.data.files.length);
  }, 10_000);

  // --- /tests ---

  test('GET /api/chorus/domain/:name/tests returns test coverage', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/tests`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.tests)).toBe(true);
    expect(body.data.byType).toBeDefined();
    expect(body._meta.count).toBeDefined();
  }, 10_000);

  // --- /alerts ---

  test('GET /api/chorus/domain/:name/alerts returns alert rules', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/alerts`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.alerts)).toBe(true);
    expect(body._meta.count).toBeDefined();
  }, 10_000);

  // --- /logs ---

  test('GET /api/chorus/domain/:name/logs returns log sources', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body._meta).toBeDefined();
  }, 10_000);

  // --- /services ---

  test('GET /api/chorus/domain/:name/services returns endpoints', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/services`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.endpoints)).toBe(true);
    expect(body._meta.count).toBeDefined();
  }, 10_000);

  // --- Consistent envelope ---

  test('all five facet endpoints use identical envelope shape', async () => {
    const facets = ['code', 'tests', 'alerts', 'logs', 'services'];
    const responses = await Promise.all(
      facets.map(f => fetch(`${API}/api/chorus/domain/seeds/${f}`).then(r => r.json()))
    );
    for (const body of responses) {
      expect(body).toHaveProperty('_meta');
      expect(body).toHaveProperty('data');
      expect(body._meta).toHaveProperty('source', 'athena');
      expect(typeof body._meta.duration_ms).toBe('number');
    }
  }, 15_000);

  // --- Blast radius consumer ---

  test('blast radius can use /code endpoint — files have path strings', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/code`);
    const body = await res.json();
    const filePaths = body.data.files.map(function(f) { return f.path; });
    expect(Array.isArray(filePaths)).toBe(true);
    expect(filePaths.length).toBeGreaterThan(0);
    expect(typeof filePaths[0]).toBe('string');
  }, 10_000);

  // --- Empty result for unknown domain ---

  test('returns empty data for unknown domain, not 500', async () => {
    const res = await fetch(`${API}/api/chorus/domain/nonexistent-xyz/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.files).toEqual([]);
  }, 10_000);
});
