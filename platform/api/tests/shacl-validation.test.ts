/**
 * SHACL validation tests — #2014
 *
 * Integration tests — hit live Athena validation API at localhost:3340.
 * Prior work: no SHACL or validation in server.ts before this card.
 * Approach: SPARQL-based constraint checking against ontology graph,
 * exposed via GET /api/athena/validate. Shapes in shapes.ttl.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('SHACL ontology validation (#2014)', () => {
  test('GET /api/athena/validate returns validation report', async () => {
    const res = await fetch(`${API}/api/athena/validate`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.valid).toBe('boolean');
    expect(Array.isArray(body.violations)).toBe(true);
    expect(Array.isArray(body.warnings)).toBe(true);
    expect(body.checked).toBeGreaterThan(0);
    expect(body.duration_ms).toBeDefined();
  }, 15_000);

  test('violations have node, constraint, severity, message', async () => {
    const res = await fetch(`${API}/api/athena/validate`);
    const body = await res.json();
    if (body.violations.length > 0) {
      const v = body.violations[0];
      expect(v).toHaveProperty('node');
      expect(v).toHaveProperty('constraint');
      expect(v.severity).toBe('violation');
      expect(v).toHaveProperty('message');
    }
  }, 15_000);

  test('warnings have severity "warning"', async () => {
    const res = await fetch(`${API}/api/athena/validate`);
    const body = await res.json();
    if (body.warnings.length > 0) {
      expect(body.warnings[0].severity).toBe('warning');
    }
  }, 15_000);

  test('validation checks all 6 constraint types', async () => {
    const res = await fetch(`${API}/api/athena/validate`);
    const body = await res.json();
    expect(body.checked).toBe(6);
  }, 15_000);
});
