/**
 * Graph separation — #1956
 * API-created entities must survive ontology reload.
 */
const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#1956: API data survives ontology reload', () => {
  test('POST actor, reload ontology, actor still exists', async () => {
    // Create
    const create = await fetch(`${API}/api/athena/subdomains/logs-service/actors`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Reload Survivor', role: 'kade', action: 'tests graph separation' }),
    });
    expect(create.status).toBe(200);

    // Reload ontology
    const reload = await fetch(`${API}/api/athena/reload`, { method: 'POST' });
    expect(reload.status).toBe(200);

    // Verify actor survives
    const get = await fetch(`${API}/api/athena/subdomains/logs-service/actors`);
    const body = await get.json();
    const survivor = body.data.actors.find(function(a) { return a.label === 'Reload Survivor'; });
    expect(survivor).toBeDefined();

    // Clean up
    await fetch(`${API}/api/athena/subdomains/logs-service/actors/logs-service-actor-reload-survivor`, { method: 'DELETE' });
  });
});
