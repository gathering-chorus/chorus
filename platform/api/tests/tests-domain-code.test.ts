/**
 * Tests domain code inventory — #2054
 *
 * The tests-domain domain page should show all test files
 * from both repos via the /code endpoint.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2054: tests-domain code endpoint', () => {
  test('returns test files from filesystem scan', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/tests-domain/code`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.test_count).toBeGreaterThan(100);
  }, 15_000);

  test('includes both gathering and chorus test files', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/tests-domain/code`);
    const body = await res.json();
    const paths = body.data.tests.map(function(f) { return f.path; });
    // Path shape: "gathering/tests/..." / "chorus/..." (was colon-separated before restructure).
    const hasGathering = paths.some(function(p) { return p.startsWith('gathering/'); });
    const hasChorus = paths.some(function(p) { return p.startsWith('chorus/'); });
    expect(hasGathering).toBe(true);
    expect(hasChorus).toBe(true);
  }, 15_000);

  test('byType includes ts and bats', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/tests-domain/code`);
    const body = await res.json();
    const types = Object.keys(body.data.byType);
    expect(types).toEqual(expect.arrayContaining(['ts', 'bats']));
  }, 15_000);
});
