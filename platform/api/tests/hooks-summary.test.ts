/**
 * Hooks summary endpoint + static page — #2099
 *
 * First per-page migration: Hooks Dashboard from Gathering EJS to Chorus
 * static page + JSON endpoint. Data sourced from chorus logs directly.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

const CATEGORIES = [
  'search-hierarchy', 'decision-gate', 'jdi-gate', 'app-state-guard',
  'sparql-guard', 'card-quality', 'deploy-gate', 'sensitive-paths',
  'credential-guard', 'ops-health', 'nudge', 'build-gate', 'permission-logger',
];

describeIntegration('#2099: /api/chorus/hooks/summary', () => {

  test('returns 200 and JSON', async () => {
    const res = await fetch(`${API}/api/chorus/hooks/summary`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 15_000);

  test('response has summaries array and totals object', async () => {
    const res = await fetch(`${API}/api/chorus/hooks/summary`);
    const body = await res.json();
    expect(Array.isArray(body.summaries)).toBe(true);
    expect(body.totals).toBeDefined();
    expect(typeof body.totals.today).toBe('number');
    expect(typeof body.totals.last7d).toBe('number');
    expect(typeof body.totals.blocks).toBe('number');
    expect(typeof body.totals.flags).toBe('number');
    expect(typeof body.totals.nudges).toBe('number');
  }, 15_000);

  test('summaries cover all 13 hook categories', async () => {
    const res = await fetch(`${API}/api/chorus/hooks/summary`);
    const body = await res.json();
    const returned = body.summaries.map(s => s.category);
    for (const cat of CATEGORIES) {
      expect(returned).toContain(cat);
    }
  }, 15_000);

  test('each summary has label, description, enforcement, counts', async () => {
    const res = await fetch(`${API}/api/chorus/hooks/summary`);
    const body = await res.json();
    const s = body.summaries[0];
    expect(s).toHaveProperty('category');
    expect(s).toHaveProperty('label');
    expect(s).toHaveProperty('description');
    expect(s).toHaveProperty('enforcement');
    expect(['enforced', 'advisory']).toContain(s.enforcement);
    expect(typeof s.today).toBe('number');
    expect(typeof s.last7d).toBe('number');
    expect(typeof s.blocks).toBe('number');
    expect(typeof s.flags).toBe('number');
    expect(typeof s.nudges).toBe('number');
    expect(Array.isArray(s.recent)).toBe(true);
  }, 15_000);
});

describeIntegration('#2099: /borg/hooks/ static page', () => {

  test('GET /borg/hooks/ returns 200', async () => {
    const res = await fetch(`${API}/borg/hooks/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page contains Governance Hooks heading and summary endpoint reference', async () => {
    const res = await fetch(`${API}/borg/hooks/`);
    const html = await res.text();
    expect(html).toContain('Governance Hooks');
    expect(html).toContain('/api/chorus/hooks/summary');
  }, 10_000);

  test('page has totals bar and category grid containers', async () => {
    const res = await fetch(`${API}/borg/hooks/`);
    const html = await res.text();
    expect(html).toContain('id="totals"');
    expect(html).toContain('id="categories"');
  }, 10_000);
});
