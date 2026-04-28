/**
 * Code inventory — #1932
 *
 * Verifies node_modules are excluded and tests array is separate.
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#1932: Code inventory excludes node_modules, splits tests', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('cards-service code inventory has fewer than 500 source files', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/cards-service/code`);
    const body = (await res.json()) as { data?: { files?: unknown[] } };
    expect((body.data?.files || []).length).toBeLessThan(500);
  });

  test('code inventory returns separate tests array', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/chorus-domain/code`);
    const body = (await res.json()) as { data?: { tests?: unknown[] } };
    expect(Array.isArray(body.data?.tests)).toBe(true);
    expect((body.data?.tests || []).length).toBeGreaterThanOrEqual(1);
  });
});
