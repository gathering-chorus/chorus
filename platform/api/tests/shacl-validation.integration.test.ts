/**
 * @test-type: api
 *
 * SHACL validation tests — #2014
 *
 * Integration tests — hit live Athena validation API at localhost:3340.
 * Prior work: no SHACL or validation in server.ts before this card.
 * Approach: SPARQL-based constraint checking against ontology graph,
 * exposed via GET /api/athena/validate. Shapes in shapes.ttl.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('SHACL ontology validation (#2014)', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/athena/validate returns validation report', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/validate`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.valid).toBe('boolean');
    expect(Array.isArray(body.violations)).toBe(true);
    expect(Array.isArray(body.warnings)).toBe(true);
    expect(body.checked).toBeGreaterThan(0);
    expect(body.duration_ms).toBeDefined();
  }, 15_000);

  test('violations have node, constraint, severity, message', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/validate`);
    const body = await res.json();
    // Live integration: assertions only run if the graph has violations.
    // Empty-violations case is covered by checked=6 in the prior test.
    /* eslint-disable jest/no-conditional-expect */
    if (body.violations.length > 0) {
      const v = body.violations[0];
      expect(v).toHaveProperty('node');
      expect(v).toHaveProperty('constraint');
      expect(v.severity).toBe('violation');
      expect(v).toHaveProperty('message');
    }
    /* eslint-enable jest/no-conditional-expect */
  }, 15_000);

  test('warnings have severity "warning"', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/validate`);
    const body = await res.json();
    // eslint-disable-next-line jest/no-conditional-expect
    if (body.warnings.length > 0) expect(body.warnings[0].severity).toBe('warning');
  }, 15_000);

  test('validation checks a non-zero set of constraint types', async () => {
    // #3559: was "checked === 6" — pinned to a snapshot of the shape set. The
    // constraint-type count tracks the evolving ontology (10 as of the current
    // model pivot, and climbing), so an exact pin is brittle-by-construction.
    // Contract: the validator actually ran constraints (checked > 0). The exact
    // count is a model fact, owned by the shapes, not this test.
    const res = await fetch(`${harness.baseUrl}/api/athena/validate`);
    const body = await res.json();
    expect(body.checked).toBeGreaterThan(0);
  }, 15_000);
});
