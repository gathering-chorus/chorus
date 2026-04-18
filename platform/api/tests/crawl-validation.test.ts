/**
 * Crawl API input validation tests — #1886
 *
 * Log evidence: GET /api/chorus/crawl/nonexistent-xyz-domain returns 200 with
 * empty arrays. Should return 404 with suggestion listing valid domains.
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('Crawl API input validation (#1886)', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /api/chorus/crawl/nonexistent returns 404', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/crawl/nonexistent-xyz-domain`);
    expect(res.status).toBe(404);
  }, 20_000);

  test('404 response includes suggestion listing valid domains', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/crawl/nonexistent-xyz-domain`);
    const body = (await res.json()) as { error?: string; suggestion?: string; valid_count?: number };
    expect(body.error).toBeDefined();
    expect(body.suggestion).toMatch(/Valid domains:/);
    expect((body.valid_count ?? 0)).toBeGreaterThan(0);
  }, 20_000);

  test('valid domain returns 200 with arrays', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/crawl/seeds`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { domain?: string; cards?: unknown[]; timeline?: unknown[] };
    expect(body.domain).toBe('seeds');
    expect(Array.isArray(body.cards)).toBe(true);
    expect(Array.isArray(body.timeline)).toBe(true);
  }, 60_000);
});
