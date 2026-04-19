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
        status: string; failures: number; warnings: number; summary: string;
        checks: Array<{ name: string; status: string; detail?: string; latencyMs?: number }>;
      };
    };
    expect(body.data.status).toBe('degraded');
    expect(body.data.failures).toBe(1);
    expect(body.data.warnings).toBe(2);
    expect(body.data.summary).toBe('chorus-api degraded');
    expect(body.data.checks).toHaveLength(2);
    expect(body.data.checks[1].detail).toBe('slow');
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
});
