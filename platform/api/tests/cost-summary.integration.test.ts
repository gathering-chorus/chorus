// @test-type: integration:api — in-process TestApp harness
/**
 * Cost summary endpoint + static page — #2099
 *
 * Per-page migration: Cost Dashboard from Gathering EJS to Chorus.
 * Aggregates claude (stats cache), twilio (API, creds-gated), clearing
 * (transcript files), tunnel (cloudflared metrics).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099: /api/chorus/cost/summary', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('returns 200 and JSON', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    expect(res.status).toBe(200);
    expect(res.headers.get('content-type')).toMatch(/json/);
  });

  test('response has claude, twilio, clearing, tunnel, summary', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    const body = await res.json();
    expect(body).toHaveProperty('claude');
    expect(body).toHaveProperty('twilio');
    expect(body).toHaveProperty('clearing');
    expect(body).toHaveProperty('tunnel');
    expect(body).toHaveProperty('summary');
  });

  test('summary has fixedCost, variableCost, totalCost', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    const body = await res.json();
    expect(typeof body.summary.fixedCost).toBe('number');
    expect(typeof body.summary.variableCost).toBe('number');
    expect(typeof body.summary.totalCost).toBe('number');
    expect(body.summary.totalCost).toBe(body.summary.fixedCost + body.summary.variableCost);
  });

  test('twilio reports pending=true when creds absent', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    const body = await res.json();
    expect(body.twilio).toHaveProperty('totalCost');
    expect(Array.isArray(body.twilio.records)).toBe(true);
    expect(typeof body.twilio.pending).toBe('boolean');
  });

  test('claude has monthlyRate and burnStatus', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    const body = await res.json();
    expect(typeof body.claude.monthlyRate).toBe('number');
    expect(['HOT', 'SMOOTH', 'COLD']).toContain(body.claude.burnStatus);
  });

  test('tunnel status is UP, DOWN, or UNKNOWN', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/cost/summary`);
    const body = await res.json();
    expect(['UP', 'DOWN', 'UNKNOWN']).toContain(body.tunnel.status);
  });
});

describe('#2099: /borg/cost/ static page', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('GET /borg/cost/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/cost/`);
    expect(res.status).toBe(200);
  });

  test('page references cost summary endpoint and shows Cost heading', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/cost/`);
    const html = await res.text();
    expect(html).toContain('Cost');
    expect(html).toContain('/api/chorus/cost/summary');
  });

  test('page has claude, twilio, clearing, tunnel panel containers', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/cost/`);
    const html = await res.text();
    expect(html).toContain('id="claude-panel"');
    expect(html).toContain('id="twilio-panel"');
    expect(html).toContain('id="clearing-panel"');
    expect(html).toContain('id="tunnel-panel"');
  });
});
