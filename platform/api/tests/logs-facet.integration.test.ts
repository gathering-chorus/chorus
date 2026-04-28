/**
 * Logs facet population tests — #2083
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

import { startTestApp, type TestApp } from './lib/test-app';


// AC1: Chorus domain subdomains have log sources mapped
describe('AC1: Chorus subdomains have log sources', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('spine-service has log sources', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/spine-service/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });

  test('chorus-domain has log sources', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/chorus-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });

  test('alerts-monitors-domain has log sources', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC2: Observability domain has monitoring log sources
describe('AC2: Observability has monitoring log sources', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain has log sources', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC3: Infrastructure domain has compute service log sources
describe('AC3: Infrastructure has compute log sources', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('infrastructure-domain has log sources', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/infrastructure-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC4: GET /api/chorus/domain/chorus/logs returns populated log sources
describe('AC4: Domain logs facet returns data', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('chorus domain logs facet returns non-empty list', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('domain-logs');
    expect(body.data.logs.length).toBeGreaterThan(0);
  });
});

// AC5: Each log source includes label, location, and status
describe('AC5: Log source metadata complete', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('chorus-domain log sources have label and location', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/chorus-domain/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.logs.length).toBeGreaterThan(0);
    const log = body.data.logs[0];
    expect(log.label).toBeTruthy();
    expect(log.location).toBeTruthy();
  });
});
