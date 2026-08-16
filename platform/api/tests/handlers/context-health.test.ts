// @test-type: unit
/**
 * context-health handler tests (#2234 Step 3).
 */

import {
  fetchContextHealth,
  type ContextHealthDeps,
} from '../../src/handlers/context-health';

function stubSparql(): ContextHealthDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextHealth', () => {
  it('healthy pulse → 200, status ok, zero counts', async () => {
    const pulse = JSON.stringify({
      health: { status: 'ok', failures: 0, warning_count: 0, summary: 'all clear', checks: [] },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { status: string; failures: number; warnings: number } };
    expect(body.data.status).toBe('ok');
    expect(body.data.failures).toBe(0);
    expect(body.data.warnings).toBe(0);
  });

  it('degraded pulse with checks → shape preserved', async () => {
    const pulse = JSON.stringify({
      health: {
        status: 'degraded',
        failures: 1,
        warning_count: 2,
        summary: 'chorus-api degraded',
        checks: [
          { name: 'chorus-api', status: 'ok', latencyMs: 12, lastCheck: '2026-04-19T10:14:50-04:00' },
          { name: 'loki', status: 'warning', detail: 'slow', lastCheck: '2026-04-19T10:14:55-04:00' },
        ],
      },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    const body = r.body as {
      data: {
        status: string; failures: number; warnings: number;
        checks: Array<{ name: string; status: string; reason?: string; latencyMs?: number }>;
      };
    };
    expect(body.data.status).toBe('degraded');
    expect(body.data.failures).toBe(1);
    expect(body.data.warnings).toBe(2);
    expect(body.data.checks).toHaveLength(2);
    expect(body.data.checks[1].reason).toBe('slow');
    expect(body.data.checks[0].latencyMs).toBe(12);
  });

  it('missing pulse → 503 with error', async () => {
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => null },
      '/api/chorus/context/health',
    );
    expect(r.status).toBe(503);
  });

  it('unparseable pulse → 500 with error', async () => {
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => '{not json' },
      '/api/chorus/context/health',
    );
    expect(r.status).toBe(500);
  });

  it('response carries no summary field — checks array speaks for itself', async () => {
    const pulse = JSON.stringify({
      health: { status: 'ok', failures: 0, warning_count: 0, summary: 'all clear', checks: [] },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    const body = r.body as { data: Record<string, unknown> };
    expect(Object.keys(body.data)).not.toContain('summary');
  });

  it('non-ok check exposes reason field, not detail', async () => {
    const pulse = JSON.stringify({
      health: {
        status: 'warning',
        failures: 0,
        warning_count: 1,
        checks: [{ name: 'loki', status: 'warning', detail: 'slow response' }],
      },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    const body = r.body as { data: { checks: Array<Record<string, unknown>> } };
    expect(body.data.checks[0]).toHaveProperty('reason', 'slow response');
    expect(body.data.checks[0]).not.toHaveProperty('detail');
  });

  it('pulse without health section → 200, defaults (unknown status, zero counts)', async () => {
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => JSON.stringify({ board: {} }) },
      '/api/chorus/context/health',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { status: string; failures: number; checks: unknown[] } };
    // 'unknown' since status is neither ok/degraded/down in input
    expect(['unknown', 'ok']).toContain(body.data.status);
    expect(body.data.failures).toBe(0);
    expect(body.data.checks).toEqual([]);
  });

  it('envelope is system-scoped (no domain / subdomain / step / product)', async () => {
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => JSON.stringify({ health: { status: 'ok' } }) },
      '/api/chorus/context/health',
    );
    expect(r.body).not.toHaveProperty('domain');
    expect(r.body).not.toHaveProperty('subdomain');
    const keys = Object.keys(JSON.parse(JSON.stringify(r.body))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
  });

  it('malformed check entries shaped without throwing', async () => {
    const pulse = JSON.stringify({
      health: {
        status: 'ok',
        checks: [null, { name: 'x' }, { name: 'y', status: 'garbage' }, { latency_ms: 5, last_check: '2026-04-19T10:14:50-04:00' }],
      },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { checks: Array<{ name: string; status: string; latencyMs?: number; lastCheck?: string }> } };
    // nulls filtered; each remaining shaped with defaults
    expect(body.data.checks).toHaveLength(3);
    expect(body.data.checks[0].name).toBe('x');
    expect(body.data.checks[1].status).toBe('unknown');
    expect(body.data.checks[2].latencyMs).toBe(5);
    expect(body.data.checks[2].lastCheck).toBe('2026-04-19T10:14:50-04:00');
  });

  // #3900 — the health surface must NAME its state. Production pulse carries
  // warnings/details as STRING ARRAYS, never `checks` — before this fix the
  // endpoint reported "degraded failures=1, checks: []" (Kade, 2026-08-15):
  // a count whose members the reader could not see.
  it('#3900: string warnings/details become named checks (the unfindable-failure fix)', async () => {
    const pulse = JSON.stringify({
      health: {
        status: 'degraded',
        failures: 1,
        warning_count: 2,
        details: ['deep-health: fuseki query timed out'],
        warnings: ['log-freshness: a.log is 46h stale', 'bare-warning-no-colon'],
      },
    });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    const body = r.body as { data: { checks: Array<{ name: string; status: string; reason?: string }> } };
    expect(body.data.checks).toHaveLength(3);
    expect(body.data.checks[0]).toEqual({ name: 'deep-health', status: 'error', reason: 'fuseki query timed out' });
    expect(body.data.checks[1]).toEqual({ name: 'log-freshness', status: 'warning', reason: 'a.log is 46h stale' });
    expect(body.data.checks[2]).toEqual({ name: 'bare-warning-no-colon', status: 'warning' });
  });

  it('#3900 negative proof: the pre-fix shape (counts, no arrays) yields empty checks — the state the fix exists to end', async () => {
    const pulse = JSON.stringify({ health: { status: 'degraded', failures: 1, warning_count: 24 } });
    const r = await fetchContextHealth(
      { sparql: stubSparql(), readPulse: () => pulse },
      '/api/chorus/context/health',
    );
    const body = r.body as { data: { failures: number; checks: unknown[] } };
    expect(body.data.failures).toBe(1);
    expect(body.data.checks).toHaveLength(0);
  });
});
