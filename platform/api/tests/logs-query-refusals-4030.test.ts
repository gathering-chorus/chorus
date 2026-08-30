// @test-type: unit — injects fetch + clock; no Loki, no live service, brings its own world.
// #4030 — the log-query tools' refusal shapes. Jeff's experience under test: when
// Loki says no, the MCP tool tells him WHY (rate-limited vs syntax vs unreachable)
// instead of an empty result, and an explicit start/end window is honoured or
// refused with the reason.
import {
  queryLogs,
  recentErrors,
  queryPainRollup,
  type LogsQueryDeps,
} from '../src/handlers/logs-query';

function fetchStatus(status: number, text = '') {
  return (async () => ({
    ok: false,
    status,
    text: async () => text,
    json: async () => ({}),
  })) as unknown as LogsQueryDeps['fetchImpl'];
}

const NOW = 1_700_000_000_000;
const base: Omit<LogsQueryDeps, 'fetchImpl'> = { lokiUrl: 'http://loki.test', now: () => NOW };

describe('logs-query refusals (#4030)', () => {
  it('Loki 429 → rate-limited, carrying Loki\'s own words', async () => {
    const r = await queryLogs({ query: '{job="x"}' }, { ...base, fetchImpl: fetchStatus(429, 'too many') });
    expect(r).toMatchObject({ ok: false, reason: 'rate-limited', detail: 'too many' });
  });

  it('Loki 5xx → loki-unreachable naming the HTTP status', async () => {
    const r = await queryLogs({ query: '{job="x"}' }, { ...base, fetchImpl: fetchStatus(502, 'bad gateway') });
    expect(r).toMatchObject({ ok: false, reason: 'loki-unreachable' });
    expect((r as { detail: string }).detail).toContain('HTTP 502');
  });

  it('an explicit start/end window is sent to Loki as nanoseconds, in order', async () => {
    let url = '';
    const fetchImpl = (async (u: string) => {
      url = u;
      return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) };
    }) as unknown as LogsQueryDeps['fetchImpl'];
    const r = await queryLogs(
      { query: '{job="x"}', start: '2026-08-30T03:00:00Z', end: '2026-08-30T05:15:54Z' },
      { ...base, fetchImpl },
    );
    expect(r.ok).toBe(true);
    expect(url).toContain(`start=${Date.parse('2026-08-30T03:00:00Z')}000000`);
    expect(url).toContain(`end=${Date.parse('2026-08-30T05:15:54Z')}000000`);
  });

  it('an unparseable or inverted start/end is refused before Loki is asked', async () => {
    let asked = 0;
    const fetchImpl = (async () => { asked++; return { ok: true, status: 200, json: async () => ({}) }; }) as unknown as LogsQueryDeps['fetchImpl'];
    const bad = await queryLogs({ query: '{job="x"}', start: 'yesterday', end: 'today' }, { ...base, fetchImpl });
    expect(bad).toMatchObject({ ok: false, detail: 'unparseable timestamp' });
    const inverted = await queryLogs(
      { query: '{job="x"}', start: '2026-08-30T05:00:00Z', end: '2026-08-30T03:00:00Z' },
      { ...base, fetchImpl },
    );
    expect(inverted).toMatchObject({ ok: false, detail: 'end must be after start' });
    expect(asked).toBe(0);
  });

  it('recentErrors for one role narrows the LogQL to that role', async () => {
    let url = '';
    const fetchImpl = (async (u: string) => {
      url = u;
      return { ok: true, status: 200, json: async () => ({ data: { result: [] } }) };
    }) as unknown as LogsQueryDeps['fetchImpl'];
    await recentErrors({ role: 'silas' }, { ...base, fetchImpl });
    const q = decodeURIComponent(url);
    expect(q).toContain('\\"role\\":\\"silas\\"');
    expect(q).toContain('\\"level\\":\\"error\\"');
  });

  it('pain rollup: 429 and 5xx from Loki are named, never an empty rollup', async () => {
    const limited = await queryPainRollup({ window: '1h' }, { ...base, fetchImpl: fetchStatus(429, 'slow down') });
    expect(limited).toMatchObject({ ok: false, reason: 'rate-limited', detail: 'slow down' });
    const down = await queryPainRollup({ window: '1h' }, { ...base, fetchImpl: fetchStatus(503, 'nope') });
    expect(down).toMatchObject({ ok: false, reason: 'loki-unreachable' });
  });
});
