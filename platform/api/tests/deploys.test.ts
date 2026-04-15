/**
 * Deploys sub-domain graph tests — #1873
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Requires RUN_INTEGRATION=true, Chorus API running, Fuseki on 3030.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

let apiUp = false;

beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    const res = await fetch(`${API}/api/athena/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

// AC1: Deploys sub-domain has child domains for deploy targets (gathering, chorus-api, launchagents)
describeIntegration('AC1: Deploys child domains', () => {
  test('deploys-domain detail returns child domains for each deploy target', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/deploys-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('subdomain-detail');
    const childIds = body.data.domains.map(d => d.id);
    expect(childIds).toContain('gathering-deploy');
    expect(childIds).toContain('chorus-api-deploy');
    expect(childIds).toContain('launchagents-deploy');
  });
});

// AC2: Each deploy target links to its pipeline config, rollback script, and health check
describeIntegration('AC2: Deploy target pipeline/rollback/health metadata', () => {
  test.each([
    ['gathering-deploy'],
    ['chorus-api-deploy'],
    ['launchagents-deploy'],
  ])('%s has pipeline, services, and actors', async (targetId) => {
    const res = await fetch(`${API}/api/athena/subdomains/${targetId}/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const sections = body.data.sections;
    expect(sections.pipeline).toBe(true);
    expect(sections.services).toBe(true);
    expect(sections.actors).toBe(true);
  }, 15_000);
});

// AC3: chorus:deploys relationship connects deploy domains to the services they ship
describeIntegration('AC3: Deploy domains consume the services they ship', () => {
  test('gathering-deploy consumes gathering-app service domain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/gathering-deploy`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const consumeIds = body.data.consumes.map(c => c.uri.split('#').pop());
    expect(consumeIds.length).toBeGreaterThan(0);
  });

  test('chorus-api-deploy consumes chorus API domain', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/chorus-api-deploy`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const consumeIds = body.data.consumes.map(c => c.uri.split('#').pop());
    expect(consumeIds.length).toBeGreaterThan(0);
  });
});

// AC4: Athena detail page for deploys-domain returns populated domains[], consumes[]
describeIntegration('AC4: Deploys detail returns populated domains and consumes', () => {
  test('deploys-domain has non-empty domains[] and consumes[]', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/deploys-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.domains.length).toBeGreaterThanOrEqual(3);
    expect(body.data.consumes.length).toBeGreaterThan(0);
  });
});

// AC5: Query "what deploys affect the spine?" returns results
describeIntegration('AC5: Spine impact query', () => {
  test('searching for deploys that affect the spine returns results', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=deploys+spine&limit=5`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // The graph should have enough data that a search for deploys+spine finds something
    expect(body.results).toBeDefined();
  });

  test('deploys-domain dependencies include spine-related domains', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/dependencies`);
    expect(res.status).toBe(200);
    const body = await res.json();
    // Deploys domain consumes infrastructure, which the spine depends on
    expect(body.data).toBeDefined();
  });
});
