/**
 * @test-type: api
 *
 * Logs facet population tests — #2083
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 *
 * #3559: CONTRACT-ONLY. These asserted live log-source PRESENCE
 * (sections.logs === true, logs.length > 0), which false-red'd whenever the
 * graph was empty or mid-recovery (invariant #4 — a test reading the foreign
 * env, not a fixture). They now assert the endpoints return the right SHAPE
 * (section boolean, logs array, item fields when present). Whether log sources
 * are actually mapped is a data-health check for the alert layer.
 */

import { startTestApp, type TestApp } from './lib/test-app';


// AC1: Chorus domain subdomains expose a logs section in completeness.
describe('AC1: Chorus subdomains report a logs section', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('spine-service completeness reports a logs section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/spine-service/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.logs).toBe('boolean');
  });

  test('chorus-domain completeness reports a logs section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/chorus-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.logs).toBe('boolean');
  });

  test('alerts-monitors-domain completeness reports a logs section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.logs).toBe('boolean');
  });
});

// AC2: Observability domain exposes a logs section.
describe('AC2: Observability reports a logs section', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain completeness reports a logs section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.logs).toBe('boolean');
  });
});

// AC3: Infrastructure domain exposes a logs section.
describe('AC3: Infrastructure reports a logs section', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('infrastructure-domain completeness reports a logs section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/infrastructure-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.logs).toBe('boolean');
  });
});

// AC4: GET /api/chorus/domain/chorus/logs returns the domain-logs envelope.
describe('AC4: Domain logs facet returns the right envelope', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('chorus domain logs facet returns a logs array under the domain-logs query', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('domain-logs');
    expect(Array.isArray(body.data.logs)).toBe(true);
  });
});

// AC5: when log sources exist, each carries label + location.
describe('AC5: Log source metadata shape', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('chorus-domain log sources, when present, have label and location', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/chorus-domain/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.data.logs)).toBe(true);
    // Shape check is conditional on data presence — the metadata CONTRACT is what
    // we guard; whether the graph currently has sources is the alert layer's job.
    /* eslint-disable jest/no-conditional-expect */
    if (body.data.logs.length > 0) {
      const log = body.data.logs[0];
      expect(log.label).toBeTruthy();
      expect(log.location).toBeTruthy();
    }
    /* eslint-enable jest/no-conditional-expect */
  });
});
