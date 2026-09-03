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
// #4079: this suite WRITES the ontology graph (discover-* INSERT what they find).
// From a test context that write is refused (#3615 membrane: tests do not write
// prod surfaces), the in-process app answers "Fuseki update 401", and the suite
// went red for 7 tests on 2026-09-02 with no product defect behind it. The suite
// runs only when a run points it at a store it MAY write (a variant store) and
// says so with CHORUS_TEST_STORE_WRITABLE=1; otherwise it is UNMEASURED, never red.
const storeWritable = process.env.CHORUS_TEST_STORE_WRITABLE === '1';
const testWhenWritable = storeWritable ? test : test.skip;
if (!storeWritable) console.warn('UNMEASURED: discover-* write the ontology graph; set CHORUS_TEST_STORE_WRITABLE=1 only against a variant store (#4079)');

describe('Discover endpoints (#2066)', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  testWhenWritable('POST /api/athena/discover-endpoints returns endpoint count > 0', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.count).toBeGreaterThan(0);
  }, 30_000);

  testWhenWritable('discovered endpoints have method, path, handler, and domainId', async () => {
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

  testWhenWritable('seed routes map to seeds-domain', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const seedRoutes = entries.filter((e: any) => e.domainId === 'seeds-domain');
    expect(seedRoutes.length).toBeGreaterThan(0);
    expect(seedRoutes.some((e: any) => e.path.includes('/seed'))).toBe(true);
  }, 30_000);

  testWhenWritable('GET /api/athena/subdomains/:id/services returns endpoints for populated domain', async () => {
    await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/seeds-domain/services`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const endpoints = body.data?.endpoints || [];
    expect(endpoints.length).toBeGreaterThan(0);
    expect(endpoints[0]).toHaveProperty('method');
    expect(endpoints[0]).toHaveProperty('path');
  }, 30_000);

  testWhenWritable('endpoints include multiple HTTP methods', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/discover-endpoints`, { method: 'POST' });
    const body = await res.json();
    const entries = body.data?.entries || [];
    const methods = new Set(entries.map((e: any) => e.method));
    expect(methods.has('GET')).toBe(true);
    expect(methods.has('POST')).toBe(true);
  }, 30_000);
});
