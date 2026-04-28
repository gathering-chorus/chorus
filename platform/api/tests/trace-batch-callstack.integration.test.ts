/**
 * trace-batch-callstack.test.ts — Batch call stack tracing (crawler/harvester)
 * Card #2102 AC: crawler emits trace hops, queryable by domain
 * Run: RUN_INTEGRATION=true npx jest tests/trace-batch-callstack.test.ts
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('Batch Call Stack Tracing (#2102)', () => {


  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('Crawler trace has 4 hops per domain (start, query, index, complete)', async () => {
    const correlationId = 'crawl-test-' + Date.now();

    for (let hop = 1; hop <= 4; hop++) {
      const services = ['crawler-start', 'crawl-api', 'sqlite-index', 'crawl-complete'];
      await fetch(harness.baseUrl + '/api/chorus/trace', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          correlationId,
          hop,
          callStack: 'batch',
          source: { domain: 'music', service: services[hop - 1] },
          destination: { domain: 'music', service: services[hop] || 'done' },
          latencyMs: hop * 100,
        }),
      });
    }

    const res = await fetch(harness.baseUrl + '/api/chorus/trace/' + correlationId);
    const data = await res.json();
    expect(data.hops).toHaveLength(4);
    expect(data.hops[0].call_stack).toBe('batch');
    expect(data.hops[3].latency_ms).toBe(400);
  });

  test('Batch traces queryable by domain via integrations endpoint', async () => {
    const res = await fetch(harness.baseUrl + '/api/chorus/trace/integrations/music');
    expect(res.status).toBe(200);
    const data = await res.json();
    const batchEdges = data.integrations.filter(function(i) { return i.call_stack === 'batch'; });
    expect(batchEdges.length).toBeGreaterThan(0);
  });

  test('Batch callStack distinguishes from integration traces', async () => {
    const correlationId = 'batch-vs-integ-' + Date.now();

    await fetch(harness.baseUrl + '/api/chorus/trace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        correlationId,
        hop: 1,
        callStack: 'batch',
        source: { domain: 'chorus', service: 'crawler' },
      }),
    });

    const res = await fetch(harness.baseUrl + '/api/chorus/trace/' + correlationId);
    const data = await res.json();
    expect(data.hops[0].call_stack).toBe('batch');
  });
});
