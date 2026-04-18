/**
 * Graph separation — #1956
 * API-created entities must survive ontology reload.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#1956: API data survives ontology reload', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('POST actor, reload ontology, actor still exists', async () => {
    // Create
    const create = await fetch(`${harness.baseUrl}/api/athena/subdomains/logs-domain/actors`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ label: 'Reload Survivor', role: 'kade', action: 'tests graph separation' }),
    });
    expect(create.status).toBe(200);

    // Reload ontology
    const reload = await fetch(`${harness.baseUrl}/api/athena/reload`, { method: 'POST' });
    expect(reload.status).toBe(200);

    // Verify actor survives
    const get = await fetch(`${harness.baseUrl}/api/athena/subdomains/logs-domain/actors`);
    const body = await get.json();
    const survivor = body.data.actors.find(function(a) { return a.label === 'Reload Survivor'; });
    expect(survivor).toBeDefined();

    // Clean up
    await fetch(`${harness.baseUrl}/api/athena/subdomains/logs-domain/actors/logs-domain-actor-reload-survivor`, { method: 'DELETE' });
  });
});
