/**
 * trace-integration-callstack.test.ts — Integration call stack tracing
 * Card #2100 AC: nudges, board events, bridge carry correlation + hops
 * Run: RUN_INTEGRATION=true npx jest tests/trace-integration-callstack.test.ts
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Integration Call Stack Tracing (#2100)', () => {

  // AC #1: Nudge delivery creates trace hops
  test('POST trace with integration callStack for nudge persist hop', async () => {
    const correlationId = `nudge-test-${Date.now()}`;
    const res = await fetch(`${API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'integration',
        source: { domain: 'chorus', service: 'nudge-persist', instance: 'silas→kade' },
        destination: { domain: 'chorus', service: 'messaging-api' },
        latencyMs: 25,
      }),
    });
    expect(res.status).toBe(200);

    // Add deliver hop
    await fetch(`${API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 2,
        callStack: 'integration',
        source: { domain: 'chorus', service: 'nudge-deliver' },
        destination: { domain: 'chorus', service: 'terminal-inject', instance: 'kade-tty' },
        latencyMs: 8,
      }),
    });

    const trace = await fetch(`${API}/api/chorus/trace/${correlationId}`);
    const data = await trace.json();
    expect(data.hops).toHaveLength(2);
    expect(data.hops[0].source_service).toBe('nudge-persist');
    expect(data.hops[1].source_service).toBe('nudge-deliver');
  });

  // AC #2: Board events carry correlation ID + hops
  test('Board event trace has correlation ID and hop metadata', async () => {
    const correlationId = `board-test-${Date.now()}`;
    const res = await fetch(`${API}/api/chorus/trace`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'integration',
        source: { domain: 'chorus', service: 'board' },
        destination: { domain: 'chorus', service: 'role-state' },
      }),
    });
    expect(res.status).toBe(200);

    const trace = await fetch(`${API}/api/chorus/trace/${correlationId}`);
    const data = await trace.json();
    expect(data.hops[0].call_stack).toBe('integration');
    expect(data.hops[0].source_service).toBe('board');
    expect(data.hops[0].dest_service).toBe('role-state');
  });

  // AC #4: Trace query shows full chain for any event
  test('Full nudge chain queryable by correlation ID', async () => {
    const correlationId = `chain-test-${Date.now()}`;

    // 3-hop nudge flow: emit → persist → deliver
    for (let hop = 1; hop <= 3; hop++) {
      const services = ['spine-emit', 'nudge-persist', 'terminal-inject'];
      await fetch(`${API}/api/chorus/trace`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          correlationId,
          hop,
          callStack: 'integration',
          source: { domain: 'chorus', service: services[hop - 1] },
          latencyMs: hop * 5,
        }),
      });
    }

    const trace = await fetch(`${API}/api/chorus/trace/${correlationId}`);
    const data = await trace.json();
    expect(data.hops).toHaveLength(3);
    expect(data.hops[0].hop).toBe(1);
    expect(data.hops[2].hop).toBe(3);

    // Total latency derivable from hops
    const totalMs = data.hops.reduce((sum, h) => sum + (h.latency_ms || 0), 0);
    expect(totalMs).toBe(30); // 5 + 10 + 15
  });

  // Integration edges include nudge services
  test('Integrations endpoint shows nudge service pairs', async () => {
    const res = await fetch(`${API}/api/chorus/trace/integrations/chorus`);
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.integrations.length).toBeGreaterThan(0);
  });
});
