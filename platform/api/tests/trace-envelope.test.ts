/**
 * trace-envelope.test.ts — Common message envelope and hop tracing
 * Card #2097 AC: envelope type, seeds instrumented, trace query, auto-populated integrations
 * Run: RUN_INTEGRATION=true npx jest tests/trace-envelope.test.ts
 */

import { startTestApp, type TestApp } from './lib/test-app';

const CHORUS_API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = process.env.RUN_INTEGRATION === 'true' ? describe : describe.skip;

describe('Common Message Envelope (#2097)', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  // AC #2: Envelope type defined — test via trace creation
  test('POST /api/chorus/trace creates a hop with all envelope fields', async () => {
    const correlationId = `test-${Date.now()}`;
    const res = await fetch(`${CHORUS_API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'integration',
        source: { domain: 'seeds', service: 'twilio-webhook', instance: '401-592-2496' },
        destination: { domain: 'seeds', service: 'app-validator' },
        latencyMs: 12,
      }),
    });
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.ok).toBe(true);
  });

  // AC #4: Trace query returns ordered hop chain
  test('GET /api/chorus/trace/:correlationId returns all hops ordered', async () => {
    const correlationId = `test-chain-${Date.now()}`;

    // Create 3 hops
    for (let hop = 1; hop <= 3; hop++) {
      await fetch(`${CHORUS_API}/api/chorus/trace`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          correlationId,
          hop,
          callStack: 'integration',
          source: { domain: 'seeds', service: `service-${hop}` },
          destination: { domain: 'seeds', service: `service-${hop + 1}` },
          latencyMs: hop * 10,
        }),
      });
    }

    const res = await fetch(`${CHORUS_API}/api/chorus/trace/${correlationId}`);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.hops).toHaveLength(3);
    expect(data.hops[0].hop).toBe(1);
    expect(data.hops[1].hop).toBe(2);
    expect(data.hops[2].hop).toBe(3);
    expect(data.hops[0].source_service).toBe('service-1');
  });

  // AC #6: Error hop carries classification
  test('Error hop carries classification (transient/permanent/validation)', async () => {
    const correlationId = `test-error-${Date.now()}`;
    const res = await fetch(`${CHORUS_API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'integration',
        source: { domain: 'seeds', service: 'twilio-webhook' },
        error: { classification: 'permanent', message: 'Twilio API returned 401', retryable: false },
      }),
    });
    expect(res.status).toBe(200);

    const trace = await fetch(`${CHORUS_API}/api/chorus/trace/${correlationId}`);
    const data = await trace.json();
    expect(data.hops[0].error_class).toBe('permanent');
    expect(data.hops[0].error_message).toContain('401');
  });

  // AC #5: Domain integrations auto-populated from traces
  test('GET /api/chorus/trace/integrations/:domain returns observed service pairs', async () => {
    const correlationId = `test-integ-${Date.now()}`;

    // Simulate a seeds integration flow
    await fetch(`${CHORUS_API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'integration',
        source: { domain: 'seeds', service: 'twilio-webhook' },
        destination: { domain: 'seeds', service: 'fuseki-graph' },
        latencyMs: 45,
      }),
    });

    const res = await fetch(`${CHORUS_API}/api/chorus/trace/integrations/seeds`);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.integrations).toBeDefined();
    expect(data.integrations.length).toBeGreaterThan(0);

    const edge = data.integrations.find(
      (i) => i.source_service === 'twilio-webhook' && i.dest_service === 'fuseki-graph'
    );
    expect(edge).toBeDefined();
  });

  // Trace for unknown correlation returns empty
  test('GET /api/chorus/trace/:unknown returns empty hops', async () => {
    const res = await fetch(`${CHORUS_API}/api/chorus/trace/nonexistent-${Date.now()}`);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.hops).toHaveLength(0);
  });
});
