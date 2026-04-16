/**
 * Interaction Patterns endpoint + static page — #2099
 *
 * Per-page migration: queries Loki for interaction.pattern.detected events,
 * aggregates by pattern and by date over a configurable window.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099: /api/chorus/patterns/summary', () => {

  test('returns 200 and JSON', async () => {
    const res = await fetch(`${API}/api/chorus/patterns/summary?days=7`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  }, 15_000);

  test('response has patterns, byDate, total, days', async () => {
    const res = await fetch(`${API}/api/chorus/patterns/summary?days=7`);
    const body = await res.json();
    expect(body.patterns).toBeDefined();
    expect(Array.isArray(body.byDate)).toBe(true);
    expect(typeof body.total).toBe('number');
    expect(body.days).toBe(7);
  }, 15_000);

  test('defaults days=30 when omitted', async () => {
    const res = await fetch(`${API}/api/chorus/patterns/summary`);
    const body = await res.json();
    expect(body.days).toBe(30);
  }, 15_000);

  test('byDate entries have date, total, counts', async () => {
    const res = await fetch(`${API}/api/chorus/patterns/summary?days=30`);
    const body = await res.json();
    if (body.byDate.length > 0) {
      const row = body.byDate[0];
      expect(row).toHaveProperty('date');
      expect(typeof row.total).toBe('number');
      expect(typeof row.counts).toBe('object');
    }
  }, 15_000);
});

describeIntegration('#2099: /borg/patterns/ static page', () => {

  test('GET /borg/patterns/ returns 200', async () => {
    const res = await fetch(`${API}/borg/patterns/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page references patterns summary endpoint and has patterns heading', async () => {
    const res = await fetch(`${API}/borg/patterns/`);
    const html = await res.text();
    expect(html).toContain('Interaction Patterns');
    expect(html).toContain('/api/chorus/patterns/summary');
  }, 10_000);

  test('page has bar, legend, and date-table containers', async () => {
    const res = await fetch(`${API}/borg/patterns/`);
    const html = await res.text();
    expect(html).toContain('id="bar"');
    expect(html).toContain('id="legend"');
    expect(html).toContain('id="date-table"');
  }, 10_000);
});
