// @test-type: integration:api — hits the running API
/**
 * (test-type moved to line 1 — #4274: a bare concern is not a layer)
 *
 * Observability domain population tests — #1963
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 *
 * #3559: CONTRACT-ONLY. These previously asserted live data PRESENCE
 * (sections.X === true), which made them lie (invariant #4): they false-red'd
 * whenever the graph was empty or mid-recovery, even though the endpoint worked.
 * They now assert the completeness endpoint returns each section as a boolean
 * (its contract). "Is the observability data actually populated?" is a
 * data-health question that belongs in the alert layer, not the nightly suite.
 */

import { startTestApp, type TestApp } from './lib/test-app';


// AC1: services section is reported (boolean) by the completeness contract.
describe('AC1: Observability services', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain completeness reports a services section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.services).toBe('boolean');
  });
});

// AC2: integrations section is reported.
describe('AC2: Observability integrations', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain completeness reports an integrations section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.integrations).toBe('boolean');
  });
});

// AC3: persistence section is reported.
describe('AC3: Observability persistence', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain completeness reports a persistence section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.persistence).toBe('boolean');
  });
});

// AC4: logs section is reported.
describe('AC4: Observability logs', () => {

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

// AC5: gaps section is reported.
describe('AC5: Observability gaps', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('observability-domain completeness reports a gaps section', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.data.sections.gaps).toBe('boolean');
  });
});

// AC6 (Sub-domains populated with children) — RETIRED #4274. It fetched the
// subdomain DETAIL route, GET /api/athena/subdomains/observability-domain, which
// went with chorus:SubDomain (#4265; subdomain-detail.sparql deleted) and 404s.
// Jeff 2026-09-23: "we are retiring subdomains". A domain's children are a model
// question answered at athena-make (:3360/domains/domains/<id>), not here. The
// completeness facets above still serve and keep their tests.

// Bonus: observability-product collapsed — no longer a SubProduct.
// Structural (a node was removed), not data-volume — kept as-is.
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
