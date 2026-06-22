/**
 * @test-type: api
 *
 * Domain pipeline view tests — #2069
 *
 * Value stream lifecycle assembled from existing data sources.
 * Shape → Design → Build → Prove → Ship per domain.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2069: domain pipeline view', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/chorus/domain/:name/pipeline returns 5 stages', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data.stages).toHaveLength(5);
    const names = body.data.stages.map(function(s) { return s.name; });
    expect(names).toEqual(['shape', 'design', 'build', 'prove', 'ship']);
  }, 15_000);

  test('each stage has status and evidence count', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/pipeline`);
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

  test('shape stage exposes a numeric evidence count', async () => {
    // #3559: was "evidence > 0" + "status !== not_started" — data-coupled to the
    // seeds domain having cards in the live graph (invariant #4). Contract: the
    // shape stage carries a numeric evidence count. The actual count is data.
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    const shape = body.data.stages.find(function(s) { return s.name === 'shape'; });
    expect(typeof shape.evidence).toBe('number');
  }, 15_000);

  test('build stage exposes evidence count + code/test/endpoint detail', async () => {
    // #3559: dropped "evidence > 0" (data-coupled); the detail KEYS are the
    // contract, the counts are data.
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    const build = body.data.stages.find(function(s) { return s.name === 'build'; });
    expect(typeof build.evidence).toBe('number');
    expect(build.detail).toHaveProperty('code');
    expect(build.detail).toHaveProperty('tests');
    expect(build.detail).toHaveProperty('endpoints');
  }, 15_000);

  test('works for domain with minimal data', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/people/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.stages).toHaveLength(5);
  }, 15_000);

  test('unknown domain returns empty pipeline, not 500', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/nonexistent-xyz/pipeline`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.stages).toHaveLength(5);
    const allNotStarted = body.data.stages.every(function(s) { return s.status === 'not_started'; });
    expect(allNotStarted).toBe(true);
  }, 15_000);

  test('uses athena envelope for consistent shape', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/pipeline`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
    expect(typeof body._meta.duration_ms).toBe('number');
  }, 15_000);
});
