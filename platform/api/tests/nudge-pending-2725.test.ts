// @test-type: unit — fixture spine log in tmpdir; no ~/.chorus, no live services.
/**
 * #2725 AC1/AC8 — GET /api/nudge/:role/pending reads the spine fold:
 * nudge.emitted minus nudge.surfaced for :role. Role-lane scoped
 * (Silas ruling 2026-08-23): a role reads only its own pending; jeff reads all.
 *
 * Fixture shapes are CAPTURED from live ~/.chorus/chorus.log 2026-08-23:
 *   nudge.emitted  → {"timestamp":"...","event":"nudge.emitted","role":"chorus-mcp",
 *                     "payload":"from=X,to=Y,chars=N,trace=T,origin=mcp,content=..."}
 *   nudge.surfaced → {"timestamp":"...","event":"nudge.surfaced","role":"pulse",
 *                     "trace_id":"T","id":N,"from":"X","to":"Y","attempt":1,...}
 * Test brings its own world (#3528): fixture log, no ~/.chorus, no live role names.
 */
import { buildNudgeFold } from '../src/nudge-fold';
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const TS = '2026-08-23T09:25:47.916-04:00';

const emitted = (trace: string, from: string, to: string, content: string) =>
  JSON.stringify({
    timestamp: TS,
    event: 'nudge.emitted',
    role: 'chorus-mcp',
    payload: `from=${from},to=${to},chars=${content.length},trace=${trace},origin=mcp,content=${content}`,
  });

const surfaced = (trace: string, from: string, to: string) =>
  JSON.stringify({
    timestamp: TS,
    event: 'nudge.surfaced',
    role: 'pulse',
    trace_id: trace,
    id: 1,
    from,
    to,
    attempt: 1,
    target: 'tmux:%1',
  });

function fixtureLog(lines: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), 'spine-2725-'));
  const p = join(dir, 'chorus.log');
  // interleave a non-nudge spine line (real logs mix event shapes)
  writeFileSync(p, ['{"appName":"chorus-events","other":true}', ...lines].join('\n') + '\n');
  return p;
}

describe('#2725 nudge pending fold', () => {
  test('AC1: emitted-without-surfaced is pending, surfaced is not', () => {
    const log = fixtureLog([
      emitted('t1', 'testerA', 'testerB', 'hi'),
      surfaced('t1', 'testerA', 'testerB'),
      emitted('t2', 'testerA', 'testerB', 'still pending'),
    ]);
    const pending = buildNudgeFold(log, 'testerB');
    expect(pending).toHaveLength(1);
    expect(pending[0]).toMatchObject({ trace: 't2', from: 'testerA', to: 'testerB', content: 'still pending' });
    expect(pending[0].ts).toBe(TS);
  });

  test('AC1 role-lane: a role sees only its own pending', () => {
    const log = fixtureLog([
      emitted('t3', 'testerA', 'testerB', 'for B'),
      emitted('t4', 'testerB', 'testerC', 'for C'),
    ]);
    expect(buildNudgeFold(log, 'testerB').map(n => n.trace)).toEqual(['t3']);
    expect(buildNudgeFold(log, 'testerC').map(n => n.trace)).toEqual(['t4']);
  });

  test('AC1 jeff-lane: all=true reads every pending lane', () => {
    const log = fixtureLog([
      emitted('t5', 'testerA', 'testerB', 'x'),
      emitted('t6', 'testerB', 'testerC', 'y'),
      surfaced('t5', 'testerA', 'testerB'),
    ]);
    expect(buildNudgeFold(log, 'jeff', { all: true }).map(n => n.trace)).toEqual(['t6']);
  });

  test('content containing commas and equals survives payload parse', () => {
    const content = 'load 70, swap=38GB — see #3989';
    const log = fixtureLog([emitted('t8', 'testerA', 'testerB', content)]);
    expect(buildNudgeFold(log, 'testerB')[0].content).toBe(content);
  });

  test('surface.failed clears the fold (Silas review: pending = emitted − surfaced − failed)', () => {
    const failed = (trace: string, from: string, to: string) =>
      JSON.stringify({ timestamp: TS, event: 'nudge.surface.failed', role: 'pulse', trace_id: trace, id: 2, from, to, attempt: 6, reason: 'no claude window found', permanent: true });
    const log = fixtureLog([
      emitted('t9', 'testerA', 'testerB', 'doomed'),
      failed('t9', 'testerA', 'testerB'),
      emitted('t10', 'testerA', 'testerB', 'alive'),
    ]);
    expect(buildNudgeFold(log, 'testerB').map(n => n.trace)).toEqual(['t10']);
  });

  test('AC8 negative proof: surfaced clears the trace regardless of line order', () => {
    const log = fixtureLog([
      surfaced('t7', 'testerA', 'testerB'),
      emitted('t7', 'testerA', 'testerB', 'zombie?'),
    ]);
    expect(buildNudgeFold(log, 'testerB')).toHaveLength(0);
  });
});
