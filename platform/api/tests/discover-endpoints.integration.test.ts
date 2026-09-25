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

  // #4292 — one literal name per case. A name built at run time
  // (`${method} ${route} ...`) cannot be read by the crawler, so these cases
  // had no Test row and their results joined nothing (crawler-validate, #4290).
  const hit = async (method: string, route: string) => {
    const res = await fetch(`${harness.baseUrl}${route}`, {
      method,
      headers: { 'Content-Type': 'application/json' },
      body: method === 'DELETE' ? undefined : JSON.stringify({ label: 'zz-4187-probe', targetId: 'zz-4187-target', path: 'zz/4187.ts' }),
    });
    const body = await res.json();
    return { status: res.status, error: body.data.error, retiredBy: body._meta.retired_by, message: String(body.data.message) };
  };
  test('POST /api/athena/subdomains answers 410 naming athena-make', async () => {
    const r = await hit('POST', '/api/athena/subdomains');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('athena-make');
  }, 30_000);
  test('PUT /api/athena/subdomains/zz-4187-probe answers 410 naming athena-make', async () => {
    const r = await hit('PUT', '/api/athena/subdomains/zz-4187-probe');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('athena-make');
  }, 30_000);
  test('POST /api/athena/subdomains/zz-4187-probe/consumes answers 410 naming athena-make', async () => {
    const r = await hit('POST', '/api/athena/subdomains/zz-4187-probe/consumes');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('athena-make');
  }, 30_000);
  test('DELETE /api/athena/subdomains/zz-4187-probe/consumes/zz-4187-target answers 410 naming athena-make', async () => {
    const r = await hit('DELETE', '/api/athena/subdomains/zz-4187-probe/consumes/zz-4187-target');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('athena-make');
  }, 30_000);
  test('POST /api/athena/subdomains/zz-4187-probe/code answers 410 naming urn:chorus:domains:code', async () => {
    const r = await hit('POST', '/api/athena/subdomains/zz-4187-probe/code');
    expect(r).toMatchObject({ status: 410, error: 'retired', retiredBy: 4187 });
    expect(r.message).toContain('urn:chorus:domains:code');
  }, 30_000);
});
