/**
 * Code inventory — #1932
 * Verifies node_modules are excluded and tests array is separate.
 */
const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#1932: Code inventory excludes node_modules, splits tests', () => {
  test('cards-service code inventory has fewer than 500 source files', async () => {
    const res = await fetch(API + '/api/athena/subdomains/cards-service/code');
    const body = await res.json();
    expect(body.data.files.length).toBeLessThan(500);
  });

  test('code inventory returns separate tests array', async () => {
    // Use chorus-domain — populated with both files and tests.
    // Was athena-domain which no longer carries scanned code in the graph.
    const res = await fetch(API + '/api/athena/subdomains/chorus-domain/code');
    const body = await res.json();
    expect(Array.isArray(body.data.tests)).toBe(true);
    expect(body.data.tests.length).toBeGreaterThanOrEqual(1);
  });
});
