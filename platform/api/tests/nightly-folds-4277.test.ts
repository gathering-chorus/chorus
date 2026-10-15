// @test-type: unit
// #4277 — /nightly reads as the current run: one banner, suites folded by test
// type in the order the run executed them, status folds inside, reds open with
// their failing cases, history last. Hermetic: renderer + pure helpers; the
// store read is a fetchFn seam; the declared-type read is a readFile seam.
import {
  parseNightlyLog, renderNightlyPage, suiteType, groupByType, oneDecimal, typeCounts,
  failingCasesQuery, parseFailingCases, hasRunTrace,
} from '../src/handlers/nightly-report';

const RUN_ONE_RED = [
  'RUN|start|2026-09-23T09:41:05',
  'test.started | system trace=nightly-1790000000000-7 units=6',
  'SUITE|lint|/chorus|kade|pass|1 pass, 0 fail (lint:ratchet clean)',
  'SUITE|coverage|platform/services/chorus-hooks|silas|pass|1 pass, 0 fail (coverage 72.53210748305766% >= floor 45%)',
  'SUITE|cargo|platform/services/werk-test|silas|pass|135 pass, 0 fail',
  'SUITE|shell|proving/scripts/tests/alert-fires-carry-evidence.test.sh|principal-silas|fail|0 pass, 1 fail',
  'SUITE|bats|platform/tests/4225-demo-fitness.bats|principal-kade|skip|0 pass, 0 fail, 6 skipped (ALL SKIPPED — not built)',
  'SUITE|coverage|platform/services/athena-validate|wren|unmeasured|0 pass, 0 fail (UNMEASURED — coverage run errored rc=101)',
  'RUN|tally|registered 8777 · ran 9467 · passed 9416 · failed 1 · unmeasured 47 · no result 30',
  'RUN|complete|2026-09-23T10:48:18',
].join('\n');

const RUN_ZERO_RED = RUN_ONE_RED
  .replace('|principal-silas|fail|0 pass, 1 fail', '|principal-silas|pass|1 pass, 0 fail')
  .replace('failed 1', 'failed 0');

const CASES = {
  'proving/scripts/tests/alert-fires-carry-evidence.test.sh': [
    { name: 'fire wrote NO evidence — a recurring fire stays undiagnosable', result: 'fail' },
  ],
};

// a readFile seam: only the shell suite has a header, bats has none
const files: Record<string, string> = {
  'proving/scripts/tests/alert-fires-carry-evidence.test.sh': '#!/bin/bash\n# @test-type: integration — runs the alert check against the live api\nset -e\n',
  'platform/tests/4225-demo-fitness.bats': '#!/usr/bin/env bats\n@test "x" { true; }\n',
};
const readFile = (p: string) => files[p] ?? null;
const typeOf = (r: { kind: string; path: string }) => suiteType(r, readFile);

describe('#4277 suite type', () => {
  it('reads the declared @test-type from a file-backed suite', () => {
    expect(suiteType({ kind: 'shell', path: 'proving/scripts/tests/alert-fires-carry-evidence.test.sh' }, readFile)).toBe('integration');
  });
  it('a suite with no declaration falls back to its tool, named so it never poses as a layer', () => {
    expect(suiteType({ kind: 'bats', path: 'platform/tests/4225-demo-fitness.bats' }, readFile)).toBe('undeclared (bats)');
  });
  it('unit-of-code kinds map to their layer; lane kinds keep their name', () => {
    expect(suiteType({ kind: 'cargo', path: 'platform/services/werk-test' }, readFile)).toBe('unit');
    expect(suiteType({ kind: 'coverage', path: 'x' }, readFile)).toBe('coverage');
    expect(suiteType({ kind: 'ui', path: 'proving/flows' }, readFile)).toBe('ui');
  });
});

describe('#4277 type groups in run order', () => {
  it('orders types by the first row that ran under each, not alphabetically', () => {
    const run = parseNightlyLog(RUN_ONE_RED)!;
    expect(groupByType(run.rows, typeOf).map((g) => g.type))
      .toEqual(['lint', 'coverage', 'unit', 'integration', 'undeclared (bats)']);
  });
});

describe('#4160 per-type counts', () => {
  it('each type fold sums its suites\' pass and fail counts', () => {
    const run = parseNightlyLog(RUN_ONE_RED)!;
    const unit = groupByType(run.rows, typeOf).find((g) => g.type === 'unit')!;
    expect(typeCounts(unit.rows)).toEqual({ pass: 135, fail: 0 });
    const page = renderNightlyPage(run, { cases: {}, typeOf });
    expect(page).toContain('<span class="lbl">integration</span><span class="hint">0 pass · 1 fail</span>');
  });
  it('negative proof: a summary without counts adds nothing, it is never read as a pass', () => {
    expect(typeCounts([{ summary: 'UNMEASURED — no output' }, { summary: '3 pass, 1 fail' }])).toEqual({ pass: 3, fail: 1 });
  });
});

describe('#4277 numbers', () => {
  it('a 14-digit coverage percentage renders with one decimal', () => {
    expect(oneDecimal('coverage 72.53210748305766% >= floor 45%')).toBe('coverage 72.5% >= floor 45%');
    expect(oneDecimal('coverage 86.75% >= floor 85%')).toBe('coverage 86.8% >= floor 85%');
  });
});

describe('#4277 the page', () => {
  const red = renderNightlyPage(parseNightlyLog(RUN_ONE_RED), { cases: CASES, typeOf });
  const green = renderNightlyPage(parseNightlyLog(RUN_ZERO_RED), { cases: {}, typeOf });

  it('one red suite: its type fold and its own fold are open, and the failing case is named', () => {
    expect(red).toContain('fire wrote NO evidence');
    const openFolds = red.match(/<details[^>]*\bopen\b/g) ?? [];
    expect(openFolds).toHaveLength(2); // the integration type fold + the red suite
  });
  it('negative proof: the same fixture with zero reds renders no open fold at all', () => {
    expect(green).not.toMatch(/<details[^>]*\bopen\b/);
    expect(green).not.toContain('fire wrote NO evidence');
  });
  it('a shell suite whose only stored row is the suite itself is not shown as a case', () => {
    const own = { 'proving/scripts/tests/alert-fires-carry-evidence.test.sh': [{ name: 'alert-fires-carry-evidence.test.sh', result: 'fail' }] };
    const page = renderNightlyPage(parseNightlyLog(RUN_ONE_RED), { cases: own, typeOf });
    expect(page).toContain('no case rows recorded');
    expect(page).not.toMatch(/class="case fail"/);
  });
  it('a red suite the store has no case rows for says so instead of showing nothing', () => {
    const page = renderNightlyPage(parseNightlyLog(RUN_ONE_RED), { cases: {}, typeOf });
    expect(page).toContain('no case rows recorded');
  });
  it('one status banner, even while the run is in progress', () => {
    const partial = renderNightlyPage(parseNightlyLog(RUN_ONE_RED.replace(/\nRUN\|complete.*$/, '')), { cases: {}, typeOf });
    expect(partial.match(/class="banner/g)).toHaveLength(1);
    expect(partial).toContain('IN PROGRESS');
  });
  it('history is the last fold on the page, after every suite', () => {
    const page = renderNightlyPage(parseNightlyLog(RUN_ONE_RED), {
      cases: {}, typeOf,
      history: [{ runId: '2026-09-23T09:41:05', completed: true, rows: parseNightlyLog(RUN_ONE_RED)!.rows }],
    });
    expect(page.indexOf('class="history"')).toBeGreaterThan(page.lastIndexOf('class="suite'));
  });
  it('an unmeasured row carries its own badge, never the pass styling', () => {
    expect(red).toMatch(/class="pill unm"[^<]*unmeasured/);
    expect(red).not.toMatch(/<li class="suite pass"[^>]*>[^<]*athena-validate/);
  });
  it('percentages on the page show one decimal', () => {
    expect(red).toContain('72.5%');
    expect(red).not.toContain('72.53210748305766');
  });
});

describe('#4277 failing cases from the store', () => {
  // #4536 — the store is Loki now: the runner writes no TestResult rows
  it('the read is bounded to the run window', () => {
    const q = failingCasesQuery({ startedAt: '2026-09-23T09:41:05', completedAt: '2026-09-23T10:48:18' });
    expect(q.start).toBe(new Date('2026-09-23T09:41:05').getTime() * 1e6);
    // a minute past completion: the last case lines land just after RUN|complete
    expect(q.end).toBe((new Date('2026-09-23T10:48:18').getTime() + 60000) * 1e6);
    expect(q.query).toContain('{job="platform-chorus", appName="chorus-events"}');
  });
  it('a run still in progress is bounded above by now, not by nothing', () => {
    const before = Date.now() * 1e6;
    const q = failingCasesQuery({ startedAt: '2026-09-23T14:34:00' });
    expect(q.end).toBeGreaterThanOrEqual(before);
  });
  it('events group by suite path, names intact, oldest first, one per case', () => {
    const e = (file: string, name: string, event = 'test.case.failed') => JSON.stringify({ event, role: 'nightly', file, case: name });
    const body = JSON.stringify({ data: { result: [{ stream: {}, values: [
      ['4', e('c.bats', 'third')],
      ['3', e('a/b.sh', 'second', 'test.case.skipped')],
      ['2', e('a/b.sh', 'name, with comma')],
      ['1', e('a/b.sh', 'name, with comma')],
    ] }] } });
    expect(parseFailingCases(body)).toEqual({
      'a/b.sh': [{ name: 'name, with comma', result: 'fail' }, { name: 'second', result: 'skip' }],
      'c.bats': [{ name: 'third', result: 'fail' }],
    });
  });
});

describe('#4536 cases are read by the run\'s own trace', () => {
  it('the run carries the trace its lines name', () => {
    expect(parseNightlyLog(RUN_ONE_RED)!.trace).toBe('nightly-1790000000000-7');
  });
  it('the Loki read filters on that trace', () => {
    const q = failingCasesQuery({ startedAt: '2026-09-23T09:41:05', trace: 'nightly-1790000000000-7' });
    expect(q.query).toContain('|= "\\"trace\\":\\"nightly-1790000000000-7\\""');
  });
  // NEGATIVE PROOF: a werk nightly overlapping the canonical run logs role
  // nightly too; its failure must not land on Jeff's page.
  it('a case from another nightly trace stays off the page', () => {
    const e = (trace: string, name: string) => JSON.stringify({ event: 'test.case.failed', role: 'nightly', trace, file: 'x.bats', case: name });
    const body = JSON.stringify({ data: { result: [{ stream: {}, values: [
      ['2', e('nightly-1790000000000-7', 'ours')],
      ['1', e('nightly-1790000000999-9', 'a werk nightly')],
    ] }] } });
    expect(parseFailingCases(body, 'nightly-1790000000000-7')).toEqual({ 'x.bats': [{ name: 'ours', result: 'fail' }] });
  });
  it('a run from before #4536 says it has no trace, never borrows a time window', () => {
    const old = RUN_ONE_RED.split('\n').filter((l) => !l.includes('trace=')).join('\n');
    const page = renderNightlyPage(parseNightlyLog(old), { cases: {} });
    expect(page).toContain('no trace (before #4536)');
    expect(hasRunTrace({ trace: 'nightly-1792056711' })).toBe(false);
    expect(hasRunTrace({ trace: 'nightly-1792049381065-39455' })).toBe(true);
  });
});

describe('#4536 the page counts what the run counted', () => {
  const e = (event: string, file: string, name: string) => JSON.stringify({ event, role: 'nightly', file, case: name });
  const body = JSON.stringify({ data: { result: [{ stream: {}, values: [
    ['1', e('test.case.failed', 'a.bats', 'broke')],
    ['2', e('test.case.skipped', 'a.bats', 'held')],
    ['3', e('test.case.passed', 'a.bats', 'mentions test.case.failed in its name')],
    ['4', e('test.case.failed', 'b.bats', 'bats-gather-tests')],
  ] }] } });
  it('fail, skip and a suite that could not list its tests stay separate; a pass never shows', () => {
    expect(parseFailingCases(body)).toEqual({
      'a.bats': [{ name: 'broke', result: 'fail' }, { name: 'held', result: 'skip' }],
      'b.bats': [{ name: 'bats-gather-tests', result: 'unmeasured' }],
    });
  });
  it('the query matches the event exactly after the cheap line filters', () => {
    expect(failingCasesQuery({ startedAt: '2026-10-15T03:00:01' }).query).toContain('| json | event=~"test.case.(failed|skipped)"');
  });
});
