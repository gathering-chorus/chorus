/**
 * Domain radius + blast radius — #2028
 *
 * Radius: what does this domain touch? (outward walk)
 * Blast radius: what depends on this domain? (inward walk)
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2028: domain radius and blast radius', () => {

  test('GET /api/chorus/domain/:name/radius returns neighborhood', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/radius`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data.edges).toBeDefined();
    expect(Array.isArray(body.data.edges)).toBe(true);
    expect(body.data.edges.length).toBeGreaterThan(0);
  }, 10_000);

  test('radius edges include relationship type', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/radius`);
    const body = await res.json();
    var edge = body.data.edges[0];
    expect(edge).toHaveProperty('target');
    expect(edge).toHaveProperty('relationship');
    expect(edge).toHaveProperty('direction');
  }, 10_000);

  test('GET /api/chorus/domain/:name/blast-radius returns impact surface', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/blast-radius`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.edges)).toBe(true);
  }, 10_000);

  test('under 200ms response time', async () => {
    const start = Date.now();
    await fetch(`${API}/api/chorus/domain/chorus/radius`);
    expect(Date.now() - start).toBeLessThan(200);
  }, 10_000);

  test('unknown domain returns empty edges', async () => {
    const res = await fetch(`${API}/api/chorus/domain/nonexistent-xyz/radius`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.edges).toEqual([]);
  }, 10_000);

  test('uses athena envelope', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/radius`);
    const body = await res.json();
    expect(body._meta.source).toBe('athena');
  }, 10_000);
});
