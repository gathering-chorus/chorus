/**
 * Timestamp consistency tests — #1826
 *
 * Bug: API responses show UTC timestamps (.toISOString()) instead of Boston time.
 * Jeff sees "2026-04-11T18:30:00.000Z" when it's 2:30pm in Boston.
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#1826: API timestamps show Boston time, not UTC', () => {
  test('freshness endpoint timestamp is not UTC ISO format', async () => {
    const res = await fetch(`${API}/api/chorus/freshness`);
    const body = await res.json() as any;
    // Bug: timestamp ends with Z (UTC) — should show Boston time
    expect(body.timestamp).toBeDefined();
    expect(body.timestamp).not.toMatch(/Z$/);
    expect(body.timestamp).not.toMatch(/\.\d{3}Z$/);
  });

  test('search endpoint timestamp is not UTC ISO format', async () => {
    const res = await fetch(`${API}/api/chorus/search?q=test&limit=1`);
    const body = await res.json() as any;
    if (body._meta?.timestamp) {
      expect(body._meta.timestamp).not.toMatch(/Z$/);
      expect(body._meta.timestamp).not.toMatch(/\.\d{3}Z$/);
    }
  });

  test('athena envelope timestamp is Boston time', async () => {
    const res = await fetch(`${API}/api/athena/health`);
    const body = await res.json() as any;
    expect(body._meta.timestamp).toBeDefined();
    // Should not end with Z (UTC) or contain T separator with Z suffix
    expect(body._meta.timestamp).not.toMatch(/\.\d{3}Z$/);
  });
});
