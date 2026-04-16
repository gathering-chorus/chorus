/**
 * Jeff dashboard endpoints + page — #2099
 *
 * Ports Gathering's /api/jeff/posture/strip + /api/werk/activity handlers
 * into chorus-api, and mounts a /borg/jeff/ page that consumes 5 existing
 * native endpoints (voice, attention, reprompt, cost) plus the two new ones.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099: /api/chorus/jeff/posture/strip', () => {

  test('returns 200 with frames, total, filtered, days', async () => {
    const res = await fetch(`${API}/api/chorus/jeff/posture/strip?days=7`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.frames)).toBe(true);
    expect(typeof body.total).toBe('number');
    expect(typeof body.filtered).toBe('number');
    expect(body.days).toBe(7);
  }, 10_000);

  test('days clamps to max 30', async () => {
    const res = await fetch(`${API}/api/chorus/jeff/posture/strip?days=100`);
    const body = await res.json();
    expect(body.days).toBe(30);
  }, 10_000);
});

describeIntegration('#2099: /api/chorus/werk/activity', () => {

  test('returns 200 with entries, total, hours, sources', async () => {
    const res = await fetch(`${API}/api/chorus/werk/activity?hours=6`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.entries)).toBe(true);
    expect(typeof body.total).toBe('number');
    expect(body.hours).toBe(6);
    expect(typeof body.sources).toBe('object');
  }, 15_000);

  test('hours clamps to [1, 168]', async () => {
    const res = await fetch(`${API}/api/chorus/werk/activity?hours=500`);
    const body = await res.json();
    expect(body.hours).toBe(168);
  }, 15_000);
});

describeIntegration('#2099: /borg/jeff/ static page', () => {

  test('GET /borg/jeff/ returns 200', async () => {
    const res = await fetch(`${API}/borg/jeff/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page references 5 chorus-api endpoints', async () => {
    const res = await fetch(`${API}/borg/jeff/`);
    const html = await res.text();
    expect(html).toContain('/api/chorus/voice-analytics');
    expect(html).toContain('/api/chorus/attention-analytics');
    expect(html).toContain('/api/chorus/reprompt-analytics');
    expect(html).toContain('/api/chorus/cost/summary');
    expect(html).toContain('/api/chorus/jeff/posture/strip');
  }, 10_000);

  test('page has posture, voice, attention, reprompt, cost, werk panel containers', async () => {
    const res = await fetch(`${API}/borg/jeff/`);
    const html = await res.text();
    expect(html).toContain('id="posture-panel"');
    expect(html).toContain('id="voice-panel"');
    expect(html).toContain('id="attention-panel"');
    expect(html).toContain('id="reprompt-panel"');
    expect(html).toContain('id="cost-panel"');
    expect(html).toContain('id="werk-panel"');
  }, 10_000);
});
