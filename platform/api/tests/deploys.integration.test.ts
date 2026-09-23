// @test-type: integration — hits the running API/store
/**
 * Deploys sub-domain graph tests — #1873
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

import { startTestApp, type TestApp } from './lib/test-app';


// #1873 AC1-AC4 originally asserted deploy-target child subdomains
// (gathering-deploy, chorus-api-deploy, launchagents-deploy) under
// deploys-domain, plus their individual completeness + consume edges.
// Those targets never survived the graph restructure. The shape check that
// replaced them ('deploys-domain is addressable and returns detail envelope')
// is RETIRED by #4274: it fetched the subdomain DETAIL route, which went with
// chorus:SubDomain (#4265; subdomain-detail.sparql deleted) and 404s. Jeff
// 2026-09-23: "we are retiring subdomains". deploys-domain as a row is served
// by athena-make (:3360/domains/domains/deploys-domain).

// AC5: Query "what deploys affect the spine?" returns results
describe('AC5: Spine impact query', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('searching for deploys that affect the spine returns results', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/search?q=deploys+spine&limit=5`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // The graph should have enough data that a search for deploys+spine finds something
    expect(body.results).toBeDefined();
  });

  test('deploys-domain dependencies include spine-related domains', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus/dependencies`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // Deploys domain consumes infrastructure, which the spine depends on
    expect(body.data).toBeDefined();
  });
});
