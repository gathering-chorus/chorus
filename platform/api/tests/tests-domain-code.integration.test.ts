/**
 * @test-type: api
 *
 * Tests domain code inventory — #2054
 *
 * The tests-domain domain page should show all test files from both repos via
 * the /code endpoint.
 *
 * Converted to in-process harness (#2173 AC4).
 *
 * #3559: CONTRACT-ONLY. "test_count > 100" / "paths include gathering/ and
 * chorus/" / "byType includes ts and bats" were coupled to a live filesystem
 * scan (invariant #4) — they false-red when the scanned tree was empty or
 * partial (e.g. a werk without sibling repos, or mid data-recovery). We now
 * assert the /code endpoint returns the right SHAPE: a numeric test_count, a
 * tests array of {path}, and a byType object. The actual file census is a
 * data/scan-health question, not a code-contract one.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2054: tests-domain code endpoint', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('returns a numeric test_count', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { _meta?: { test_count?: number } };
    expect(typeof body._meta?.test_count).toBe('number');
  }, 15_000);

  test('returns a tests array of path-bearing entries', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    const body = (await res.json()) as { data?: { tests?: Array<{ path: string }> } };
    const tests = body.data?.tests || [];
    expect(Array.isArray(tests)).toBe(true);
    // eslint-disable-next-line jest/no-conditional-expect -- shape check only when scan returned files
    if (tests.length > 0) expect(typeof tests[0].path).toBe('string');
  }, 15_000);

  test('returns a byType breakdown object', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/subdomains/tests-domain/code`);
    const body = (await res.json()) as { data?: { byType?: Record<string, unknown> } };
    expect(typeof body.data?.byType).toBe('object');
    expect(body.data?.byType).not.toBeNull();
  }, 15_000);
});
