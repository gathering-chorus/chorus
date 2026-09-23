// @test-type: unit
// #4277 — /nightly reads as the current run: one banner, suites folded by test
// type in the order the run executed them, status folds inside, reds open with
// their failing cases, history last. Hermetic: renderer + pure helpers; the
// store read is a fetchFn seam; the declared-type read is a readFile seam.
import {
  parseNightlyLog, renderNightlyPage, suiteType, groupByType, oneDecimal,
  failingCasesQuery, parseFailingCases,
} from '../src/handlers/nightly-report';

const RUN_ONE_RED = [
  'RUN|start|2026-09-23T09:41:05',
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
  it('the query is bounded to the run window and excludes passes', () => {
    const q = failingCasesQuery({ startedAt: '2026-09-23T09:41:05', completedAt: '2026-09-23T10:48:18' });
    expect(q).toContain('"2026-09-23T09:41:05"');
    expect(q).toContain('"2026-09-23T10:48:18"');
    expect(q).toContain('?res != "pass"');
  });
  it('a run still in progress is bounded above by the far future, not by nothing', () => {
    expect(failingCasesQuery({ startedAt: '2026-09-23T14:34:00' })).toContain('"9999"');
  });
  it('csv rows group by suite path, quoted names intact', () => {
    const csv = 'fp,tn,res\r\na/b.sh,"name, with comma",fail\r\na/b.sh,second,skip\r\nc.bats,third,fail\r\n';
    expect(parseFailingCases(csv)).toEqual({
      'a/b.sh': [{ name: 'name, with comma', result: 'fail' }, { name: 'second', result: 'skip' }],
      'c.bats': [{ name: 'third', result: 'fail' }],
    });
  });
});
