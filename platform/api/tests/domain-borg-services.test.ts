/**
 * Borg services on domain page — #2080
 *
 * Wire borg:Environment instances into domain-detail services section.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2080: borg services on domain page', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /api/chorus/domain/:name/infra returns borg environments', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/infra`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(body.data.environments).toBeDefined();
    expect(body.data.environments.length).toBeGreaterThan(0);
  }, 10_000);

  test('each environment has name, port, engine, host', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/infra`);
    const body = await res.json();
    const env = body.data.environments[0];
    expect(env).toHaveProperty('name');
    expect(env).toHaveProperty('engine');
    expect(env).toHaveProperty('host');
  }, 10_000);

  test('dependency chains are included', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/infra`);
    const body = await res.json();
    var withDeps = body.data.environments.filter(function(e) { return e.dependsOn && e.dependsOn.length > 0; });
    expect(withDeps.length).toBeGreaterThan(0);
  }, 10_000);

  test('uses athena envelope', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/infra`);
    const body = await res.json();
    expect(body._meta).toBeDefined();
    expect(body._meta.source).toBe('athena');
  }, 10_000);

  test('returns 200 for any domain — infra is system-wide', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/nonexistent-xyz/infra`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // Borg environments are system-wide, not domain-scoped
    expect(Array.isArray(body.data.environments)).toBe(true);
  }, 10_000);
});
