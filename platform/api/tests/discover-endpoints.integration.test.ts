// @test-type: integration — NEGATIVE PROOF (#3734) that a retired catch-all writer answers 410; starts the in-process app, writes nothing
/**
 * #4187 — the SubDomain hand-edit routes that wrote urn:chorus:instances are
 * retired (create, update, consumes add/remove, code create). Each answers 410
 * and names athena-make's generated route or the crawler. Restoring any write
 * body turns its case red. The discovery routes' proof lives in
 * discover-pages.integration.test.ts.
 */
import { startTestApp, TestApp } from './lib/test-app';
import { withServiceAuth } from './lib/service-token';

withServiceAuth();

describe('#4187 — the SubDomain catch-all writers are retired', () => {
  let harness: TestApp;
  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  const cases: Array<[string, string, string]> = [
    ['POST', '/api/athena/subdomains', 'athena-make'],
    ['PUT', '/api/athena/subdomains/zz-4187-probe', 'athena-make'],
    ['POST', '/api/athena/subdomains/zz-4187-probe/consumes', 'athena-make'],
    ['DELETE', '/api/athena/subdomains/zz-4187-probe/consumes/zz-4187-target', 'athena-make'],
    ['POST', '/api/athena/subdomains/zz-4187-probe/code', 'urn:chorus:domains:code'],
  ];
  for (const [method, route, names] of cases) {
    test(`${method} ${route} answers 410 naming ${names}`, async () => {
      const res = await fetch(`${harness.baseUrl}${route}`, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: method === 'DELETE' ? undefined : JSON.stringify({ label: 'zz-4187-probe', targetId: 'zz-4187-target', path: 'zz/4187.ts' }),
      });
      expect(res.status).toBe(410);
      const body = await res.json();
      expect(body.data.error).toBe('retired');
      expect(body.data.message).toContain(names);
      expect(body._meta.retired_by).toBe(4187);
    }, 30_000);
  }
});
