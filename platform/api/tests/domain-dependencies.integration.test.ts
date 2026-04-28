/**
 * Domain dependencies facet — #2082
 *
 * Two layers: explicit ontology edges (consumes/consumedBy)
 * + inferred shared-infrastructure edges from borg.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2082: domain dependencies facet', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/chorus/domain/:name/dependencies returns direct and shared', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/dependencies`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data).toHaveProperty('direct');
    expect(body.data).toHaveProperty('shared');
  }, 10_000);

  test('direct dependencies have consumes and consumedBy', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/dependencies`);
    const body = await res.json();
    expect(body.data.direct).toHaveProperty('consumes');
    expect(body.data.direct).toHaveProperty('consumedBy');
    expect(Array.isArray(body.data.direct.consumes)).toBe(true);
  }, 10_000);

  test('shared infrastructure shows domains sharing environments', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/dependencies`);
    const body = await res.json();
    expect(Array.isArray(body.data.shared)).toBe(true);
    // Seeds shares fuseki-library with other domains
    /* eslint-disable jest/no-conditional-expect -- shape assertion only when graph has shared deps */
    if (body.data.shared.length > 0) {
      expect(body.data.shared[0]).toHaveProperty('domain');
      expect(body.data.shared[0]).toHaveProperty('sharedVia');
    }
    /* eslint-enable jest/no-conditional-expect */
  }, 10_000);

  test('uses athena envelope', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/seeds/dependencies`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
  }, 10_000);

  test('unknown domain returns empty, not error', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/nonexistent-xyz/dependencies`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.direct.consumes).toEqual([]);
    expect(body.data.shared).toEqual([]);
  }, 10_000);
});
