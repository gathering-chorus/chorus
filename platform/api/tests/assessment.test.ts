/**
 * Borg Assessment endpoint + static page — #2099
 *
 * Per-page migration: Borg Assessment shows capability maturity across
 * 5 value-stream buckets × ~25 domains with codebase/harvest/graph status.
 * Topology data is proxied from Gathering (lives in its RDF store there).
 * Churn panel degrades gracefully until admin-auth path is sorted.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099: /api/chorus/codebase/topology proxy', () => {

  test('returns 200 and JSON when Gathering is up', async () => {
    const res = await fetch(`${API}/api/chorus/codebase/topology`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 15_000);

  test('response has summary, spokes, domains', async () => {
    const res = await fetch(`${API}/api/chorus/codebase/topology`);
    const body = await res.json();
    expect(body.summary).toBeDefined();
    expect(body.spokes).toBeDefined();
    expect(body.domains).toBeDefined();
  }, 15_000);
});

describeIntegration('#2099: /borg/assessment/ static page', () => {

  test('GET /borg/assessment/ returns 200', async () => {
    const res = await fetch(`${API}/borg/assessment/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page has Borg Assessment heading and topology endpoint', async () => {
    const res = await fetch(`${API}/borg/assessment/`);
    const html = await res.text();
    expect(html).toContain('Borg Assessment');
    expect(html).toContain('/api/chorus/codebase/topology');
  }, 10_000);

  test('page has grid body + value-stream sections', async () => {
    const res = await fetch(`${API}/borg/assessment/`);
    const html = await res.text();
    expect(html).toContain('id="grid-body"');
    expect(html).toContain('Sowing');
    expect(html).toContain('Reflecting');
  }, 10_000);
});
