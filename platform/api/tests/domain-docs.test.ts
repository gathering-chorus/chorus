/**
 * Domain docs proxy tests — #2078
 *
 * Proxies doc-catalog tagged entries through consolidated domain API.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const APP = 'http://localhost:3000';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2078: domain docs proxy', () => {

  test('GET /api/chorus/domain/:name/docs returns tagged docs', async () => {
    const res = await fetch(`${APP}/api/chorus/domain/seeds-domain/docs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.governs).toBeDefined();
    expect(body.governs.length).toBeGreaterThan(0);
  }, 10_000);

  test('docs have title, type, href, and tags', async () => {
    const res = await fetch(`${APP}/api/chorus/domain/chorus-domain/docs`);
    const body = await res.json();
    const doc = body.governs[0];
    expect(doc).toHaveProperty('title');
    expect(doc).toHaveProperty('type');
    expect(doc).toHaveProperty('href');
    expect(doc).toHaveProperty('tags');
  }, 10_000);

  test('chorus has 50+ tagged docs', async () => {
    const res = await fetch(`${APP}/api/chorus/domain/chorus-domain/docs`);
    const body = await res.json();
    expect(body.governs.length).toBeGreaterThanOrEqual(50);
  }, 10_000);

  test('unknown domain returns empty, not error', async () => {
    const res = await fetch(`${APP}/api/chorus/domain/nonexistent-xyz/docs`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.governs).toEqual([]);
  }, 10_000);
});
