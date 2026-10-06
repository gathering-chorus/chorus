// @test-type: integration:api — the spine-event route through the test app; temp spine log, temp index db
// @domain: events
// @card: #2109
// @owner: wren
/**
 * spine-event-endpoint.test.ts — Spine event service endpoint
 * Card #2109 AC: POST /api/chorus/spine-event accepts events, auto-traces hops
 * Run: RUN_INTEGRATION=true npx jest tests/spine-event-endpoint.test.ts
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('POST /api/chorus/spine-event (#2109)', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('accepts a spine event with envelope fields', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        event: 'seed.received',
        role: 'system',
        domain: 'seeds',
        source_service: 'twilio-webhook',
        trace_id: `test-spine-${Date.now()}`,
      }),
    });
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.ok).toBe(true);
    // #4417 — it lands in the test's own spine, never ~/.chorus/chorus.log.
    // NEGATIVE PROOF: drop CHORUS_LOG_FILE from tests/lib/test-app.ts and the
    // path check below fails (the route falls back to the live spine).
    const log = process.env.CHORUS_LOG_FILE as string;
    expect(require('fs').realpathSync(require('path').dirname(log)).startsWith(require('fs').realpathSync(require('os').tmpdir()))).toBe(true);
    expect(require('fs').readFileSync(log, 'utf8')).toContain('test-spine-');
  });

  test('event with hop field auto-creates trace entry', async () => {
    const traceId = `test-spine-hop-${Date.now()}`;
    await fetch(`${harness.baseUrl}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        event: 'seed.received',
        role: 'system',
        domain: 'seeds',
        source_service: 'twilio-webhook',
        dest_service: 'app-validator',
        trace_id: traceId,
        hop: 1,
      }),
    });

    const trace = await fetch(`${harness.baseUrl}/api/chorus/trace/${traceId}`);
    const data = await trace.json();
    expect(data.hops.length).toBeGreaterThan(0);
    expect(data.hops[0].source_service).toBe('twilio-webhook');
  });

  test('event without hop does not create trace entry', async () => {
    const traceId = `test-spine-nohop-${Date.now()}`;
    await fetch(`${harness.baseUrl}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        event: 'seed.received',
        role: 'system',
        trace_id: traceId,
      }),
    });

    const trace = await fetch(`${harness.baseUrl}/api/chorus/trace/${traceId}`);
    const data = await trace.json();
    expect(data.hops).toHaveLength(0);
  });

  test('missing event field returns 400', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ role: 'system' }),
    });
    expect(res.status).toBe(400);
  });
});
