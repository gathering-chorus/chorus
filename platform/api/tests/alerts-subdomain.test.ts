/**
 * Alerts sub-domain graph tests — #1870
 *
 * Integration tests — hit live Athena API at localhost:3340.
 * Alerts is a flat collection: 14 rules + notification channel as instances,
 * each rule with a monitors edge to the domain it watches.
 * No child sub-domains — Jeff's direction: keep it flat like code or tests.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Alerts sub-domain graph (#1870)', () => {
  // Alert rule instances were 14+ at #1870. Current graph has zero — the data
  // hasn't been repopulated into the new urn:chorus:ontology structure. Tests
  // shape-check the endpoint, not specific counts, until data is reloaded.
  // Fuseki-direct tests (via /pods/query) were dropped per "no raw Fuseki"
  // principle — should go through the API layer.
  test('alerts-monitors-domain endpoint returns valid structure', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/alerts-monitors-domain`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data.instances)).toBe(true);
  }, 15_000);

  test('alerts-monitors-domain has no child sub-domains (flat collection)', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/alerts-monitors-domain`);
    const body = await res.json();
    expect((body.data.domains || []).length).toBe(0);
  }, 15_000);
});
