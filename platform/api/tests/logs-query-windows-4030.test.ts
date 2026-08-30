// @test-type: unit — injects fetch + clock; no Loki, brings its own world.
// #4030 — every card/trace/branch log tool refuses a bad window before asking
// Loki, with the same reason, so Jeff never sees an empty result for a typo.
import { logsForCard, logsForTrace, logsForBranch, type LogsQueryDeps } from '../src/handlers/logs-query';
import { runEventloopProbe } from '../src/eventloop-probe';

describe('log tools refuse a bad window up front (#4030)', () => {
  it('card / trace / branch: "fortnight" is refused and Loki is never asked', async () => {
    let asked = 0;
    const deps: LogsQueryDeps = {
      lokiUrl: 'http://loki.test', now: () => 1_700_000_000_000,
      fetchImpl: (async () => { asked++; return { ok: true, status: 200, json: async () => ({}) }; }) as unknown as LogsQueryDeps['fetchImpl'],
    };
    const bad = 'fortnight' as any;
    const rs = await Promise.all([
      logsForCard({ card_id: 4030, time_window: bad }, deps),
      logsForTrace({ trace_id: 't-1', time_window: bad }, deps),
      logsForBranch({ branch: 'silas/4030', time_window: bad }, deps),
    ]);
    for (const r of rs) expect(r).toMatchObject({ ok: false, reason: 'time-range-invalid' });
    expect(asked).toBe(0);
  });
});

describe('eventloop probe default pacing (#4030)', () => {
  it('with no sleep injected the loop paces itself with a real timer and still stops at its tick budget', async () => {
    const emitted: unknown[] = [];
    const t0 = Date.now();
    await runEventloopProbe({
      probe: async () => ({ latencyMs: 1, timedOut: false }),
      emit: (a) => emitted.push(a),
      nudge: () => {},
      ticks: 2,
      intervalMs: 5,
    });
    expect(emitted).toEqual([]);
    expect(Date.now() - t0).toBeGreaterThanOrEqual(8);
  });
});
