// @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
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
    if (!r.ok) throw new Error('expected ok result'); // narrow; unreachable past the assert above
    expect(r.events[0].branch).toBe('kade/3023'); // AC3: actual branch recorded + parsed
    expect(r.events[0].card_id).toBe(3023);        // AC5: chain key still present
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

// --- #3606 — pain-rollup + per-key query coverage (was the 47-stmt uncovered
// region behind the platform/api coverage red). rollupRawLines is pure by
// design; fixtures drive it, no Loki.
import { rollupRawLines, windowToSeconds, queryPainRollup } from '../src/handlers/logs-query';

describe('windowToSeconds (#3029 v1.1)', () => {
  it('parses <n>[smhd]; rejects everything else', () => {
    expect(windowToSeconds('30m')).toBe(1800);
    expect(windowToSeconds('6h')).toBe(21600);
    expect(windowToSeconds('7d')).toBe(604800);
    expect(windowToSeconds('45s')).toBe(45);
    expect(windowToSeconds('1w')).toBeNull();
    expect(windowToSeconds('h')).toBeNull();
  });
});

function painBody(values: Array<[string, string]>) {
  return { data: { resultType: 'streams', result: [{ values }] } };
}
const pline = (ts: string, o: Record<string, unknown>): [string, string] => [ts, JSON.stringify(o)];

describe('rollupRawLines (#3029/#3165)', () => {
  it('groups by role·event·reason, counts, ranks by count desc, tracks cards + latest detail', () => {
    const body = painBody([
      pline('1000000000000000000', { event: 'demo.show.failed', role: 'kade', reason: 'no_demo_started', card_id: 3606 }),
      pline('3000000000000000000', { event: 'demo.show.failed', role: 'kade', reason: 'no_demo_started', card_id: 3600, detail: 'latest detail line' }),
      pline('2000000000000000000', { event: 'werk.merge.refused', role: 'wren', reason: 'dirty', card_id: 3603 }),
    ]);
    const { classes, total } = rollupRawLines(body);
    expect(total).toBe(3);
    expect(classes).toHaveLength(2);
    expect(classes[0]).toMatchObject({ role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started', count: 2, cards: ['3600', '3606'] });
    expect(classes[0].detail).toContain('latest detail line'); // detail follows the LATEST ns
    expect(classes[1].count).toBe(1);
  });

  it('drops synthetic test cards, unparseable lines, and role-filtered rows', () => {
    const body = painBody([
      pline('1', { event: 'x.failed', role: 'kade', card_id: 99998 }), // synthetic
      ['2', 'not json at all'],
      pline('3', { event: 'y.failed', role: 'wren' }),
      pline('4', { event: 'z.failed', role: 'kade' }),
    ]);
    const { classes, total } = rollupRawLines(body, { role: 'kade' });
    expect(total).toBe(1);
    expect(classes[0].event).toBe('z.failed');
  });

  it('attributes products by event family and resolves domain via the injected board resolver', () => {
    const body = painBody([
      pline('1', { event: 'crawler.graph.failed', role: 'system', card_id: 1320 }),
      pline('2', { event: 'werk.build.failed', role: 'kade', card_id: 3606 }),
    ]);
    const { classes, byProduct } = rollupRawLines(body, {}, (cardId) => (cardId === '3606' ? 'chorus' : 'photos'));
    expect(byProduct).toEqual({ Gathering: 1, Chorus: 1 });
    const crawl = classes.find((c) => c.event === 'crawler.graph.failed');
    expect(crawl).toMatchObject({ product: 'Gathering', domain: 'photos' });
  });
});

describe('queryPainRollup (#3029)', () => {
  it('refuses bad windows without touching Loki', async () => {
    let fetched = 0;
    const deps: LogsQueryDeps = { ...baseDeps, fetchImpl: (async () => { fetched++; return {} as Response; }) as unknown as LogsQueryDeps['fetchImpl'] };
    const r = await queryPainRollup({ window: 'fortnight' }, deps);
    expect(r).toMatchObject({ ok: false, reason: 'time-range-invalid' });
    expect(fetched).toBe(0);
  });

  it('rolls up a successful Loki response', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: async () => ({
        ok: true, status: 200,
        json: async () => painBody([pline('1000000000000000000', { event: 'a.failed', role: 'silas', reason: 'boom' })]),
      }) as unknown as Response,
    };
    const r = await queryPainRollup({ window: '1d' }, deps);
    expect(r).toMatchObject({ ok: true, total: 1 });
    expect((r as { classes: unknown[] }).classes[0]).toMatchObject({ role: 'silas', event: 'a.failed', reason: 'boom' });
  });

  it('maps Loki 400 to query-syntax-error', async () => {
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: async () => ({ ok: false, status: 400, text: async () => 'parse error' }) as unknown as Response,
    };
    const r = await queryPainRollup({ window: '1d' }, deps);
    expect(r).toMatchObject({ ok: false, reason: 'query-syntax-error' });
  });
});

describe('logsForCard / logsForBranch — JSON-field anchored queries', () => {
  it('logsForCard builds a card_id field anchor and returns rows', async () => {
    let seenUrl = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        seenUrl = String(url);
        return { ok: true, status: 200, json: async () => painBody([pline('1', { event: 'werk.landed', role: 'kade', card_id: 3606 })]) } as unknown as Response;
      }) as LogsQueryDeps['fetchImpl'],
    };
    const r = await logsForCard({ card_id: 3606 }, deps);
    expect(r.ok).toBe(true);
    expect(decodeURIComponent(seenUrl)).toContain('card_id');
    expect(decodeURIComponent(seenUrl)).toContain('3606');
  });

  it('logsForBranch anchors on the branch field', async () => {
    let seenUrl = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        seenUrl = String(url);
        return { ok: true, status: 200, json: async () => painBody([]) } as unknown as Response;
      }) as LogsQueryDeps['fetchImpl'],
    };
    const r = await logsForBranch({ branch: 'kade/3606' }, deps);
    expect(r.ok).toBe(true);
    expect(decodeURIComponent(seenUrl)).toContain('branch');
    expect(decodeURIComponent(seenUrl)).toContain('kade/3606');
  });
});

describe("time_window '7d' (#3621 — a card's trace outlives 1d)", () => {
  it('logsForTrace with 7d builds a week-wide range', async () => {
    let seenUrl = '';
    const deps: LogsQueryDeps = {
      ...baseDeps,
      fetchImpl: (async (url: string) => {
        seenUrl = String(url);
        return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) } as unknown as Response;
      }) as LogsQueryDeps['fetchImpl'],
    };
    const r = await logsForTrace({ trace_id: 't-7d', time_window: '7d' }, deps);
    expect(r.ok).toBe(true);
    const u = new URL(seenUrl);
    const spanNs = BigInt(u.searchParams.get('end')!) - BigInt(u.searchParams.get('start')!);
    expect(spanNs).toBe(BigInt(7 * 86400) * 1000000000n);
  });
});
