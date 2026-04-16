/**
 * Fitness Functions endpoint + static page — #2099
 *
 * Per-page migration: Fitness Functions from Gathering EJS to Chorus.
 * Four per-role metrics computed from chorus.log: JDI rate, decision-gate
 * rate, search-hierarchy rate, retry-cluster rate.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

const EXPECTED_IDS = ['jdi-rate', 'decision-gate-rate', 'search-hierarchy-rate', 'retry-rate'];

describeIntegration('#2099: /api/chorus/fitness/summary', () => {

  test('returns 200 and JSON', async () => {
    const res = await fetch(`${API}/api/chorus/fitness/summary`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 15_000);

  test('response has functions array', async () => {
    const res = await fetch(`${API}/api/chorus/fitness/summary`);
    const body = await res.json();
    expect(Array.isArray(body.functions)).toBe(true);
    expect(body.functions.length).toBe(4);
  }, 15_000);

  test('functions cover all 4 fitness metrics', async () => {
    const res = await fetch(`${API}/api/chorus/fitness/summary`);
    const body = await res.json();
    const ids = body.functions.map(f => f.id);
    for (const id of EXPECTED_IDS) {
      expect(ids).toContain(id);
    }
  }, 15_000);

  test('each function has byRole, trend7d, overall7d, direction', async () => {
    const res = await fetch(`${API}/api/chorus/fitness/summary`);
    const body = await res.json();
    const f = body.functions[0];
    expect(f).toHaveProperty('id');
    expect(f).toHaveProperty('label');
    expect(f).toHaveProperty('description');
    expect(['lower-is-better', 'higher-is-better']).toContain(f.direction);
    expect(f).toHaveProperty('byRole');
    expect(typeof f.trend7d).toBe('number');
    expect(typeof f.overall7d).toBe('number');
    expect(typeof f.overallToday).toBe('number');
    expect(Array.isArray(f.recentEvents)).toBe(true);
  }, 15_000);

  test('byRole covers silas, wren, kade', async () => {
    const res = await fetch(`${API}/api/chorus/fitness/summary`);
    const body = await res.json();
    const f = body.functions[0];
    expect(f.byRole).toHaveProperty('silas');
    expect(f.byRole).toHaveProperty('wren');
    expect(f.byRole).toHaveProperty('kade');
    expect(typeof f.byRole.silas.rate).toBe('number');
    expect(typeof f.byRole.silas.sessions).toBe('number');
  }, 15_000);
});

describeIntegration('#2099: /borg/fitness/ static page', () => {

  test('GET /borg/fitness/ returns 200', async () => {
    const res = await fetch(`${API}/borg/fitness/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page contains Fitness Functions heading and summary endpoint', async () => {
    const res = await fetch(`${API}/borg/fitness/`);
    const html = await res.text();
    expect(html).toContain('Fitness Functions');
    expect(html).toContain('/api/chorus/fitness/summary');
  }, 10_000);

  test('page has functions container', async () => {
    const res = await fetch(`${API}/borg/fitness/`);
    const html = await res.text();
    expect(html).toContain('id="functions"');
  }, 10_000);
});
