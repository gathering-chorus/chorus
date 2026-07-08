// @test-type: integration — hits the live chorus-api at :3340; carries a scoped service token on writes (#3619).
/**
 * Discover API endpoints per domain — #2066
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Prior work: follows discover-code (#1868) and discover-pages (#2065) pattern.
 */

import { startTestApp, type TestApp } from './lib/test-app';
import { withServiceAuth } from './lib/service-token';
// #3619 — live mutation endpoints are envelope-secured; this suite is a real
// consumer and carries a scoped token on every write (deploy-before-require).
withServiceAuth();

describe('Discover endpoints (#2066)', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('POST /api/athena/discover-endpoints returns endpoint count > 0', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThan(0);
  }, 30_000);

  test('discovered endpoints have method, path, handler, and domainId', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    expect(entries.length).toBeGreaterThan(0);
    const ep = entries[0];
    expect(ep).toHaveProperty('method');
    expect(ep).toHaveProperty('path');
    expect(ep).toHaveProperty('handler');
    expect(ep).toHaveProperty('domainId');
  }, 30_000);

  test('seed routes map to seeds-domain', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const seedRoutes = entries.filter((e: any) => e.domainId === 'seeds-domain');
    expect(seedRoutes.length).toBeGreaterThan(0);
    expect(seedRoutes.some((e: any) => e.path.includes('/seed'))).toBe(true);
  }, 30_000);

  test('GET /api/athena/subdomains/:id/services returns endpoints for populated domain', async () => {
    await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/seeds-domain/services`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const endpoints = body.data?.endpoints || [];
    expect(endpoints.length).toBeGreaterThan(0);
    expect(endpoints[0]).toHaveProperty('method');
    expect(endpoints[0]).toHaveProperty('path');
  }, 30_000);

  test('endpoints include multiple HTTP methods', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const methods = new Set(entries.map((e: any) => e.method));
    expect(methods.has('GET')).toBe(true);
    expect(methods.has('POST')).toBe(true);
  }, 30_000);
});
