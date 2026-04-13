/**
 * Crawl API input validation tests — #1886
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Log evidence: GET /api/chorus/crawl/nonexistent-xyz-domain returns 200
 * with empty arrays. Should return 404.
 *
 * Prior work: crawl endpoint (#1956) traverses OWL + cards + spine.
 * No domain validation — returns partial empty results for any string.
 * Approach: check requested domain against Athena subdomain list,
 * return 404 with suggestion for unknown domains.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Crawl API input validation (#1886)', () => {
  test('GET /api/chorus/crawl/nonexistent returns 404', async () => {
    const res = await fetch(`${API}/api/chorus/crawl/nonexistent-xyz-domain`);
    expect(res.status).toBe(404);
  }, 20_000);

  test('404 response includes suggestion to use /api/athena/subdomains', async () => {
    const res = await fetch(`${API}/api/chorus/crawl/nonexistent-xyz-domain`);
    const body = await res.json();
    expect(body.error).toBeDefined();
    expect(body.suggestion).toMatch(/subdomains/);
  }, 20_000);

  test('valid domain returns 200 with arrays', async () => {
    // Use a small domain — 'chorus' has 100+ cards and times out on per-card --json calls
    const res = await fetch(`${API}/api/chorus/crawl/convergence`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.domain).toBe('convergence');
    expect(Array.isArray(body.cards)).toBe(true);
    expect(Array.isArray(body.timeline)).toBe(true);
  }, 60_000);
});
