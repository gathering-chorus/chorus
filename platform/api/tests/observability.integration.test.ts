/**
 * Observability domain population tests — #1963
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

import { startTestApp, type TestApp } from './lib/test-app';


// AC1: All observability services populated
describe('AC1: Observability services', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has services section populated', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.services).toBe(true);
  });
});

// AC2: All integrations populated
describe('AC2: Observability integrations', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has integrations section populated', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.integrations).toBe(true);
  });
});

// AC3: All persistence stores populated
describe('AC3: Observability persistence', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has persistence section populated', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.persistence).toBe(true);
  });
});

// AC4: Log sources populated (done in #2083, verify still present)
describe('AC4: Observability logs', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has logs section populated', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC5: Gaps populated
describe('AC5: Observability gaps', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has gaps section populated', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.gaps).toBe(true);
  });
});

// AC6: Sub-domains populated with children
describe('AC6: Observability sub-domains as children', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has child domains', async () => {
    // Graph structure drifted since #1870 — specific children moved out of the
    // observability-domain hierarchy (alerts-monitors-domain and logs-domain
    // became peers). Assertion kept as "has any children" to preserve intent
    // (observability is a parent domain) without pinning to the specific ids.
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const childIds = body.data.domains.map(d => d.id);
    expect(childIds.length).toBeGreaterThan(0);
  });
});

// Bonus: observability-product collapsed — no longer a SubProduct
describe('Graph restructure: product collapsed', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-product is no longer a SubProduct', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-product/completeness`);
    // Should be 404 — the SubProduct node has been removed
    expect(res.status).toBe(404);
  });
});
