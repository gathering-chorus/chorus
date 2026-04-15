/**
 * Domain pipeline view tests — #2069
 *
 * Value stream lifecycle assembled from existing data sources.
 * Shape → Design → Build → Prove → Ship per domain.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2069: domain pipeline view', () => {

  test('GET /api/chorus/domain/:name/pipeline returns 5 stages', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data.stages).toHaveLength(5);
    const names = body.data.stages.map(function(s) { return s.name; });
    expect(names).toEqual(['shape', 'design', 'build', 'prove', 'ship']);
  }, 15_000);

  test('each stage has status and evidence count', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    for (const stage of body.data.stages) {
      expect(stage).toHaveProperty('name');
      expect(stage).toHaveProperty('status');
      expect(['not_started', 'in_progress', 'complete']).toContain(stage.status);
      expect(stage).toHaveProperty('evidence');
      expect(typeof stage.evidence).toBe('number');
      expect(stage).toHaveProperty('detail');
    }
  }, 15_000);

  test('shape stage reflects card count', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    const shape = body.data.stages.find(function(s) { return s.name === 'shape'; });
    expect(shape.evidence).toBeGreaterThan(0);
    expect(shape.status).not.toBe('not_started');
  }, 15_000);

  test('build stage reflects code + test + endpoint counts', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    const build = body.data.stages.find(function(s) { return s.name === 'build'; });
    expect(build.evidence).toBeGreaterThan(0);
    expect(build.detail).toHaveProperty('code');
    expect(build.detail).toHaveProperty('tests');
    expect(build.detail).toHaveProperty('endpoints');
  }, 15_000);

  test('works for domain with minimal data', async () => {
    const res = await fetch(`${API}/api/chorus/domain/people/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.stages).toHaveLength(5);
  }, 15_000);

  test('unknown domain returns empty pipeline, not 500', async () => {
    const res = await fetch(`${API}/api/chorus/domain/nonexistent-xyz/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.stages).toHaveLength(5);
    const allNotStarted = body.data.stages.every(function(s) { return s.status === 'not_started'; });
    expect(allNotStarted).toBe(true);
  }, 15_000);

  test('uses athena envelope for consistent shape', async () => {
    const res = await fetch(`${API}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
    expect(typeof body._meta.duration_ms).toBe('number');
  }, 15_000);
});
