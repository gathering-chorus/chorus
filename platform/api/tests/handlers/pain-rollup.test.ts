/**
 * pain rollup (#3029) — unit tests for the shared rollup query + parser.
 *
 * Locks the validated query contract into code so the front end / MCP tool can
 * never silently revert to the bug class:
 *   - {job=~".+"} selector, never appName (appName is blind to job-labeled spine
 *     events, returns 0 for whole classes).
 *   - event-field anchor via json + event=~, never a bare substring filter
 *     (substring over-counts ~6x).
 *   - aggregates by role,event,reason, ranked by count, window threaded.
 * Cross-checked live 2026-05-21: this grouping reproduces the board exactly.
 */
import { buildRollupQuery, parseRollupVector, eventMatchesPainFilter, PAIN_EVENT_SUFFIXES } from '../../src/handlers/logs-query';

const VECTOR_FIXTURE = {
  data: {
    result: [
      { metric: { role: 'silas', event: 'session.context.error' }, value: [0, '283'] },
      { metric: { role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started' }, value: [0, '432'] },
      { metric: { role: 'wren', event: 'mcp.tool.error' }, value: [0, '53'] },
      { metric: { role: 'x', event: 'noise' }, value: [0, '0'] },
    ],
  },
};

const TOP_CLASS = { role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started', count: 432 };

describe('buildRollupQuery — validated contract is locked in code', () => {
  it('uses the job selector and never appName', () => {
    const q = buildRollupQuery('7d');
    expect(q).toContain('job=~');
    expect(q).not.toContain('appName');
  });

  it('anchors on the event field via json, never a bare substring filter', () => {
    const q = buildRollupQuery('7d');
    expect(q).toContain('| json');
    expect(q).toContain('event=~');
    expect(q).not.toContain('|=');
  });

  it('aggregates by role, event, reason and threads the window', () => {
    const q = buildRollupQuery('7d');
    expect(q).toContain('sum by (role, event, reason)');
    expect(q).toContain('count_over_time');
    expect(q).toContain('[7d]');
  });

  it('threads each supported window into the query', () => {
    expect(buildRollupQuery('1d')).toContain('[1d]');
    expect(buildRollupQuery('7d')).toContain('[7d]');
    expect(buildRollupQuery('30d')).toContain('[30d]');
  });
});

describe('parseRollupVector — grouping and ranking', () => {
  it('parses an instant vector, drops zero-count rows, ranks by count desc', () => {
    const classes = parseRollupVector(VECTOR_FIXTURE);
    expect(classes.map((c) => c.count)).toEqual([432, 283, 53]);
    expect(classes[0]).toEqual(TOP_CLASS);
    expect(classes[1].reason).toBe('');
  });

  it('is empty-safe on a malformed or empty body', () => {
    expect(parseRollupVector({})).toEqual([]);
    expect(parseRollupVector({ data: { result: [] } })).toEqual([]);
    expect(parseRollupVector(null)).toEqual([]);
  });
});

describe('AC6 — filter excludes the heartbeat/probe false-positives, counts match a direct aggregation', () => {
  // Faithful fixture: REAL event names + volumes observed in live Loki 2026-05-21.
  // The original substring filter false-counted these high-volume non-failures;
  // the event-suffix anchor must drop them. heartbeat.probe (482) ~= the ~484/day FP.
  type RawEv = { role: string; event: string; reason?: string };
  const RAW: RawEv[] = [
    ...Array<RawEv>(482).fill({ role: 'system', event: 'heartbeat.probe' }),
    ...Array<RawEv>(6398).fill({ role: 'system', event: 'system.heartbeat' }),
    ...Array<RawEv>(1099).fill({ role: 'system', event: 'clearing.probe.passed' }),
    ...Array<RawEv>(3).fill({ role: 'grafana', event: 'datasource.health.ok' }), // grafana self-log
    ...Array<RawEv>(12).fill({ role: 'system', event: 'clearing.probe.failed' }),
    ...Array<RawEv>(8).fill({ role: 'kade', event: 'demo.show.failed', reason: 'no_demo_started' }),
    ...Array<RawEv>(5).fill({ role: 'wren', event: 'mcp.tool.error' }),
    ...Array<RawEv>(2).fill({ role: 'kade', event: 'card.commit.refused' }),
  ];

  it('excludes heartbeat / probe-passed / grafana self-logs, keeps only real failures', () => {
    const keptEvents = new Set(RAW.filter((r) => eventMatchesPainFilter(r.event)).map((r) => r.event));
    // the 484-FP and its high-volume non-failure friends are gone
    expect(keptEvents.has('heartbeat.probe')).toBe(false);
    expect(keptEvents.has('system.heartbeat')).toBe(false);
    expect(keptEvents.has('clearing.probe.passed')).toBe(false);
    expect(keptEvents.has('datasource.health.ok')).toBe(false);
    // real failures survive — including the .refused and .error siblings
    expect(keptEvents.has('clearing.probe.failed')).toBe(true);
    expect(keptEvents.has('demo.show.failed')).toBe(true);
    expect(keptEvents.has('mcp.tool.error')).toBe(true);
    expect(keptEvents.has('card.commit.refused')).toBe(true);
  });

  it('grouped-then-summed total equals the direct sum (mirrors the live 305==305 invariant)', () => {
    const kept = RAW.filter((r) => eventMatchesPainFilter(r.event));
    const direct = kept.length;
    // group by role,event,reason then sum the buckets back (sum(sum by(...)))
    const groups = new Map<string, number>();
    for (const r of kept) {
      const k = `${r.role}|${r.event}|${r.reason ?? ''}`;
      groups.set(k, (groups.get(k) ?? 0) + 1);
    }
    const grouped = [...groups.values()].reduce((s, n) => s + n, 0);
    expect(grouped).toBe(direct);
    // 27 real failures counted; the 7982 heartbeat/probe/grafana lines dropped
    expect(direct).toBe(12 + 8 + 5 + 2);
  });
});

describe('drift guard — the LogQL query and the JS predicate share one rule', () => {
  it('buildRollupQuery embeds every PAIN_EVENT_SUFFIXES token via the json+event anchor, never substring', () => {
    const q = buildRollupQuery('7d');
    for (const s of PAIN_EVENT_SUFFIXES) expect(q).toContain(s); // .failed / .refused / .error
    expect(q).toContain('| json');
    expect(q).toContain('event=~');
    expect(q).not.toContain('|=');
  });
});
