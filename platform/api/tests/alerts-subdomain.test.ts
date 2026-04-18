/**
 * Alerts sub-domain graph tests — #1870
 *
 * Alerts is a flat collection: 14 rules + notification channel as instances,
 * each rule with a monitors edge to the domain it watches.
 * No child sub-domains — Jeff's direction: keep it flat like code or tests.
 *
 * Converted to in-process harness (#2173 AC4). Still requires live Fuseki
 * on 3030 (handlers hit SPARQL directly); mocking at the handler seam lands
 * with Silas's decomposition design.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('Alerts sub-domain graph (#1870)', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('alerts-monitors-domain endpoint returns valid structure', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { data?: { instances?: unknown[] } };
    expect(body.data).toBeDefined();
    expect(Array.isArray(body.data?.instances)).toBe(true);
  }, 15_000);

  test('alerts-monitors-domain has no child sub-domains (flat collection)', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/alerts-monitors-domain`);
    const body = (await res.json()) as { data?: { domains?: unknown[] } };
    expect((body.data?.domains || []).length).toBe(0);
  }, 15_000);
});
