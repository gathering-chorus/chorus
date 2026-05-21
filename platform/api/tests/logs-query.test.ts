import {
  queryLogs,
  recentErrors,
  logsForCard,
  logsForTrace,
  logsForBranch,
  type LogsQueryDeps,
} from '../src/handlers/logs-query';

function makeFetch(streams: Array<{ values: Array<[string, string]> }>) {
  return async () => ({
    ok: true,
    status: 200,
    json: async () => ({ data: { resultType: 'streams', result: streams } }),
  }) as unknown as Response;
}

function lokiLine(event: string, fields: Record<string, unknown>): [string, string] {
  return [String(Date.now() * 1_000_000), JSON.stringify({ event, ts: new Date().toISOString(), ...fields })];
}

const baseDeps: LogsQueryDeps = {
  fetchImpl: makeFetch([]),
  lokiUrl: 'http://localhost:3102',
  now: () => 1_700_000_000_000,
};

describe('chorus_logs_query (#2840)', () => {
  it('returns structured rows with count + truncated flag on a successful query', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: makeFetch([
        {
          values: [
            lokiLine('chorus_acp.invoked', { role: 'silas', card_id: 2840, trace_id: 't-1' }),
            lokiLine('chorus_acp.completed', { role: 'silas', card_id: 2840, trace_id: 't-1' }),
          ],
        },
      ]),
    };
    const r = await queryLogs({ query: '{job="chorus-api"}', limit: 10 }, deps);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.events).toHaveLength(2);
    expect(r.events[0]).toMatchObject({ event: 'chorus_acp.invoked', card_id: 2840, trace_id: 't-1' });
    expect(r.count).toBe(2);
    expect(r.truncated).toBe(false);
  });

  it('refuses with loki-unreachable when fetch throws', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: async () => { throw new Error('connect ECONNREFUSED'); },
    };
    const r = await queryLogs({ query: '{job="chorus-api"}' }, deps);
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.reason).toBe('loki-unreachable');
  });

  it('refuses with query-syntax-error on Loki 400', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: async () => ({
        ok: false, status: 400,
        text: async () => 'parse error: unexpected token',
      }) as unknown as Response,
    };
    const r = await queryLogs({ query: '{bogus' }, deps);
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.reason).toBe('query-syntax-error');
  });

  it('marks truncated=true when result count equals requested limit', async () => {
    const lines = Array.from({ length: 5 }, (_, i) => lokiLine(`evt.${i}`, { role: 'silas' }));
    const deps: LogsQueryDeps = { ...baseDeps, fetchImpl: makeFetch([{ values: lines }]) };
    const r = await queryLogs({ query: '{job="chorus-api"}', limit: 5 }, deps);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.truncated).toBe(true);
  });
});

describe('chorus_logs_for_trace (#2840)', () => {
  it('builds the right LogQL filter for trace_id', async () => {
    let captured = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        captured = url;
        return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) } as unknown as Response;
      }) as unknown as typeof fetch,
    };
    await logsForTrace({ trace_id: '019e0d76-3109-74f8-8404-f2e5af337901' }, deps);
    expect(captured).toContain('019e0d76-3109-74f8-8404-f2e5af337901');
    // #2860 — query is anchored on the JSON envelope shape (\"trace_id\":\"<uuid>\"),
    // not a bare UUID substring. This was the behavior before #2860; pure
    // substring matching pulled in nudge bodies + tool_call telemetry that
    // happen to quote the UUID, distorting per-trace counts (Wren verify
    // on #2840 found 23 vs 13 — the gap was substring leakage).
    expect(decodeURIComponent(captured)).toContain('trace_id\\":\\"019e0d76');
  });

  it('returns events sorted with terminal-event detection if present', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: makeFetch([{
        values: [
          lokiLine('chorus_acp.completed', { trace_id: 't-1', card_id: 9 }),
          lokiLine('chorus_acp.invoked', { trace_id: 't-1', card_id: 9 }),
        ],
      }]),
    };
    const r = await logsForTrace({ trace_id: 't-1' }, deps);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.events.length).toBe(2);
  });
});

describe('chorus_logs_for_card (#2840)', () => {
  it('builds LogQL filter on card_id JSON field', async () => {
    let captured = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        captured = url;
        return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) } as unknown as Response;
      }) as unknown as typeof fetch,
    };
    await logsForCard({ card_id: 2840 }, deps);
    // LogQL string contains escaped quotes (\"card_id\":2840) when decoded
    expect(decodeURIComponent(captured)).toContain('card_id\\":2840');
  });
});

describe('chorus_logs_for_branch (#3023)', () => {
  it('builds LogQL filter anchored on the branch JSON field', async () => {
    let captured = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        captured = url;
        return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) } as unknown as Response;
      }) as unknown as typeof fetch,
    };
    await logsForBranch({ branch: 'kade/3023' }, deps);
    // AC4: branch is a queryable key — structural JSON-field anchor (mirrors trace_id)
    expect(decodeURIComponent(captured)).toContain('branch\\":\\"kade/3023');
  });

  it('parses branch off the row (AC3) while preserving the card_id chain key (AC5)', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: makeFetch([{ values: [lokiLine('build.push.completed', { branch: 'kade/3023', card_id: 3023 })] }]),
    };
    const r = await logsForBranch({ branch: 'kade/3023' }, deps);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.events[0].branch).toBe('kade/3023'); // AC3: actual branch recorded + parsed
      expect(r.events[0].card_id).toBe(3023);        // AC5: chain key still present
    }
  });
});

describe('chorus_logs_recent_errors (#2840)', () => {
  it('filters to error level and applies time_window', async () => {
    let captured = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        captured = url;
        return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) } as unknown as Response;
      }) as unknown as typeof fetch,
    };
    await recentErrors({ time_window: '1h' }, deps);
    // LogQL string with escaped quotes for the JSON substring match
    expect(decodeURIComponent(captured)).toContain('level\\":\\"error');
  });
});

describe('non-JSON / file-tailed lines (#3031)', () => {
  it('keeps raw non-JSON lines (nightly SUITE|, coverage) instead of silently dropping them', async () => {
    const tsNs = String(1_700_000_000_000 * 1_000_000);
    const rawLine =
      'SUITE|bats|/Users/jeffbridwell/CascadeProjects/chorus/platform/tests/bedroom-health.bats|silas|pass|bats: 4 passed, 0 failed';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: makeFetch([
        {
          values: [
            [tsNs, rawLine], // file-tailed raw line — the old catch{return null} dropped this
            lokiLine('chorus_commit.refused', { role: 'kade', card_id: 3031 }), // JSON spine line
          ],
        },
      ]),
    };
    const r = await queryLogs({ query: '{job="daemon-logs"}', limit: 10 }, deps);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    // BOTH survive — the raw SUITE| line is no longer dropped (the #3031 bug: 224 raw vs 0 here)
    expect(r.events).toHaveLength(2);
    const raw = r.events.find((e) => e.raw !== undefined);
    expect(raw?.raw).toBe(rawLine);
    // raw lines carry Loki's own entry ts (they have no embedded ts), not new Date()
    expect(raw?.ts).toBe(new Date(1_700_000_000_000).toISOString());
    // JSON spine lines still parse into structured fields (no regression)
    const spine = r.events.find((e) => e.event === 'chorus_commit.refused');
    expect(spine).toMatchObject({ role: 'kade', card_id: 3031 });
  });
});
