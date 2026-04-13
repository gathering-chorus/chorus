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
  test('alerts-service has 14+ alert rule instances', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/alerts-service`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const instances = body.data.instances || [];
    expect(instances.length).toBeGreaterThanOrEqual(14);
  }, 15_000);

  test('alerts-service has no child sub-domains (flat collection)', async () => {
    const res = await fetch(`${API}/api/athena/subdomains/alerts-service`);
    const body = await res.json();
    expect((body.data.domains || []).length).toBe(0);
  }, 15_000);

  test('each alert rule maps to a domain via monitors edge', async () => {
    const query = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?alert ?label WHERE { GRAPH <urn:chorus:ontology> { ?alert chorus:monitors ?target . ?alert a chorus:AlertRule . ?alert rdfs:label ?label } }';
    const res = await fetch(`http://localhost:3030/pods/query?query=${encodeURIComponent(query)}`, {
      headers: { 'Accept': 'application/json' },
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.results.bindings.length).toBe(14);
  }, 15_000);

  test('"what alerts cover knowledge?" returns results', async () => {
    const query = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?alert ?label WHERE { GRAPH <urn:chorus:ontology> { ?alert chorus:monitors chorus:knowledge-domain . ?alert rdfs:label ?label } }';
    const res = await fetch(`http://localhost:3030/pods/query?query=${encodeURIComponent(query)}`, {
      headers: { 'Accept': 'application/json' },
    });
    const body = await res.json();
    expect(body.results.bindings.length).toBeGreaterThan(0);
  }, 15_000);
});
