/**
 * Tests domain code inventory — #2054
 *
 * The tests-domain domain page should show all test files from both repos via
 * the /code endpoint.
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2054: tests-domain code endpoint', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('returns test files from filesystem scan', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { _meta?: { test_count?: number } };
    expect(body._meta?.test_count).toBeGreaterThan(100);
  }, 15_000);

  test('includes both gathering and chorus test files', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    const body = (await res.json()) as { data?: { tests?: Array<{ path: string }> } };
    const paths = (body.data?.tests || []).map((f) => f.path);
    expect(paths.some((p) => p.startsWith('gathering/'))).toBe(true);
    expect(paths.some((p) => p.startsWith('chorus/'))).toBe(true);
  }, 15_000);

  test('byType includes ts and bats', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    const body = (await res.json()) as { data?: { byType?: Record<string, unknown> } };
    const types = Object.keys(body.data?.byType || {});
    expect(types).toEqual(expect.arrayContaining(['ts', 'bats']));
  }, 15_000);
});
