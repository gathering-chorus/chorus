/**
 * spine-event-endpoint.test.ts — Spine event service endpoint
 * Card #2109 AC: POST /api/chorus/spine-event accepts events, auto-traces hops
 * Run: RUN_INTEGRATION=true npx jest tests/spine-event-endpoint.test.ts
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('POST /api/chorus/spine-event (#2109)', () => {

  test('accepts a spine event with envelope fields', async () => {
    const res = await fetch(`${API}/api/chorus/spine-event`, {
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
  });

  test('event with hop field auto-creates trace entry', async () => {
    const traceId = `test-spine-hop-${Date.now()}`;
    await fetch(`${API}/api/chorus/spine-event`, {
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

    const trace = await fetch(`${API}/api/chorus/trace/${traceId}`);
    const data = await trace.json();
    expect(data.hops.length).toBeGreaterThan(0);
    expect(data.hops[0].source_service).toBe('twilio-webhook');
  });

  test('event without hop does not create trace entry', async () => {
    const traceId = `test-spine-nohop-${Date.now()}`;
    await fetch(`${API}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        event: 'test.nohop',
        role: 'system',
        trace_id: traceId,
      }),
    });

    const trace = await fetch(`${API}/api/chorus/trace/${traceId}`);
    const data = await trace.json();
    expect(data.hops).toHaveLength(0);
  });

  test('missing event field returns 400', async () => {
    const res = await fetch(`${API}/api/chorus/spine-event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ role: 'system' }),
    });
    expect(res.status).toBe(400);
  });
});
