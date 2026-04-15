/**
 * Logs facet population tests — #2083
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

// AC1: Chorus domain subdomains have log sources mapped
describeIntegration('AC1: Chorus subdomains have log sources', () => {
  test('spine-service has log sources', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/spine-service/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });

  test('chorus-domain has log sources', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/chorus-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });

  test('alerts-monitors-domain has log sources', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/alerts-monitors-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC2: Observability domain has monitoring log sources
describeIntegration('AC2: Observability has monitoring log sources', () => {
  test('observability-domain has log sources', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/observability-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC3: Infrastructure domain has compute service log sources
describeIntegration('AC3: Infrastructure has compute log sources', () => {
  test('infrastructure-domain has log sources', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/infrastructure-domain/completeness`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.sections.logs).toBe(true);
  });
});

// AC4: GET /api/chorus/domain/chorus/logs returns populated log sources
describeIntegration('AC4: Domain logs facet returns data', () => {
  test('chorus domain logs facet returns non-empty list', async () => {
    const res = await fetch(`${API}/api/chorus/domain/chorus/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body._meta.query_name).toBe('domain-logs');
    expect(body.data.logs.length).toBeGreaterThan(0);
  });
});

// AC5: Each log source includes label, location, and status
describeIntegration('AC5: Log source metadata complete', () => {
  test('chorus-domain log sources have label and location', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/chorus-domain/logs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data.logs.length).toBeGreaterThan(0);
    const log = body.data.logs[0];
    expect(log.label).toBeTruthy();
    expect(log.location).toBeTruthy();
  });
});
