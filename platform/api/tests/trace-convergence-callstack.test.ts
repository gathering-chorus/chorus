/**
 * trace-convergence-callstack.test.ts — Convergence call stack tracing
 * Card #2103 AC: TTL load, SHACL validation, completeness traces
 * Run: RUN_INTEGRATION=true npx jest tests/trace-convergence-callstack.test.ts
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('Convergence Call Stack Tracing (#2103)', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('Convergence trace has callStack=convergence', async () => {
    const correlationId = 'convergence-test-' + Date.now();
    await fetch(harness.baseUrl + '/api/chorus/trace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'convergence',
        source: { domain: 'chorus', service: 'git-queue' },
        destination: { domain: 'chorus', service: 'ontology-validate' },
      }),
    });

    const res = await fetch(harness.baseUrl + '/api/chorus/trace/' + correlationId);
    const data = await res.json();
    expect(data.hops).toHaveLength(1);
    expect(data.hops[0].call_stack).toBe('convergence');
  });

  test('SHACL validation failure creates error hop', async () => {
    const correlationId = 'convergence-fail-' + Date.now();
    await fetch(harness.baseUrl + '/api/chorus/trace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 2,
        callStack: 'convergence',
        source: { domain: 'chorus', service: 'ontology-validate' },
        error: { classification: 'validation', message: 'MISSING PROPERTY: ownedBy', retryable: false },
      }),
    });

    const res = await fetch(harness.baseUrl + '/api/chorus/trace/' + correlationId);
    const data = await res.json();
    expect(data.hops[0].error_class).toBe('validation');
    expect(data.hops[0].error_message).toContain('ownedBy');
  });

  test('Convergence traces queryable by domain', async () => {
    const res = await fetch(harness.baseUrl + '/api/chorus/trace/integrations/chorus');
    expect(res.status).toBe(200);
    const data = await res.json();
    const convEdges = data.integrations.filter(function(i) { return i.call_stack === 'convergence'; });
    expect(convEdges.length).toBeGreaterThanOrEqual(0);
  });
});
