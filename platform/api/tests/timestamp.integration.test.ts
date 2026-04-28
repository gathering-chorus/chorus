/**
 * Timestamp consistency tests — #1826
 *
 * Bug: API responses show UTC timestamps (.toISOString()) instead of Boston time.
 * Jeff sees "2026-04-11T18:30:00.000Z" when it's 2:30pm in Boston.
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#1826: API timestamps show Boston time, not UTC', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('freshness endpoint timestamp is not UTC ISO format', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/freshness`);
    const body = (await res.json()) as { timestamp?: string };
    expect(body.timestamp).toBeDefined();
    expect(body.timestamp).not.toMatch(/Z$/);
    expect(body.timestamp).not.toMatch(/\.\d{3}Z$/);
  });

  test('athena envelope timestamp is Boston time', async () => {
    const res = await fetch(`${harness.baseUrl}/api/athena/health`);
    const body = (await res.json()) as { _meta?: { timestamp?: string } };
    expect(body._meta?.timestamp).toBeDefined();
    expect(body._meta?.timestamp).not.toMatch(/\.\d{3}Z$/);
  });
});
