// @test-type: unit
// @domain: tests — placed by chorus-crawl's domain rules (#4538)
// #4155 — "all failures and exceptions caught and logged as tests run".
//
// On 2026-09-12 two 500s, five TypeErrors and two 503s sat in raw jest text in
// the flat log and the readout said "exceptions 0". The runner now reads each
// failed case's reason, writes it on the case's result row (failureReason) and
// counts the run's errors into a RUN|errors line. This file proves the page and
// the readout Jeff receives carry both. Hermetic: renderer + parsers over the
// store's CSV and the log format the runner writes.
import {
  parseNightlyLog, renderNightlyPage, failingCasesQuery, parseFailingCases,
} from '../src/handlers/nightly-report';
import { parseAllRuns, buildReadout, renderReadoutText } from '../src/handlers/nightly-readout';

const ERRORS_LINE = 'RUN|errors|failed cases 3 · exceptions 1 · http 1 · assertions 0 · other 1';
const RUN_LINES = [
  'RUN|start|2026-09-25T06:00:05',
  'test.started | system trace_id=nightly-1790000000000-42 trace=nightly-1790000000000-42 units=2',
  'SUITE|npm|platform/api|kade|fail|Tests: 2 failed, 198 passed',
  'SUITE|shell|platform/scripts/test-x.sh|kade|fail|0 pass, 1 fail',
  ERRORS_LINE,
  'RUN|complete|2026-09-25T06:40:05|suites=2',
];
const RUN = RUN_LINES.join('\n');
const RUN_NO_ERRORS = RUN_LINES.filter((l) => l !== ERRORS_LINE).join('\n');

const TYPE_ERROR = 'TypeError: Cannot read properties of undefined (reading foo)';
const HTTP_500 = 'Error: expect(received).toBe(expected) · Expected: 200 · Received: 500';
const SHELL_503 = 'FAIL: curl said 503 Service Unavailable';

// #4536 — Loki's answer to failingCasesLogQL: the runner's test.case.failed
// events (results live in Loki now, not TestResult rows)
const ev = (file: string, name: string, reason: string): string =>
  JSON.stringify({ event: 'test.case.failed', role: 'nightly', trace: 'nightly-1790000000000-42', file, case: name, ...(reason ? { reason } : {}) });
const csvOf = (withReasons: boolean): string => JSON.stringify({ data: { result: [{ stream: {}, values: [
  ['1', ev('platform/api/tests/a.test.ts', 'throws a TypeError', withReasons ? TYPE_ERROR : '')],
  ['2', ev('platform/api/tests/a.test.ts', 'harness answers 500', withReasons ? HTTP_500 : '')],
  ['3', ev('platform/api/tests/a.test.ts', 'an old row from before 4155', '')],
  ['4', ev('platform/scripts/test-x.sh', 'platform/scripts/test-x.sh', withReasons ? SHELL_503 : '')],
] }] } });

describe('#4155 the store read carries the reason', () => {
  it('reads the nightly role\'s failed and skipped case events from Loki', () => {
    const q = failingCasesQuery({ startedAt: '2026-09-25T06:00:05' }).query;
    expect(q).toContain('test.case.(failed|skipped)');
    expect(q).toContain('|= "\\"role\\":\\"nightly\\""');
  });
  it('keeps each case reason and leaves a reasonless row without one', () => {
    const cases = parseFailingCases(csvOf(true))['platform/api/tests/a.test.ts'];
    expect(cases.map((c) => c.reason)).toEqual([TYPE_ERROR, HTTP_500, undefined]);
  });
});

describe('#4155 the page shows why each case failed', () => {
  const page = renderNightlyPage(parseNightlyLog(RUN), { cases: parseFailingCases(csvOf(true)) });

  it('the TypeError and the 500 each appear under their case', () => {
    expect(page).toContain(TYPE_ERROR);
    expect(page).toContain('Received: 500');
  });
  it('a shell suite with no case rows still shows its last lines', () => {
    expect(page).toContain(SHELL_503);
  });
  it('the banner carries the run error counts', () => {
    expect(page).toContain('errors</span> failed cases 3 · exceptions 1 · http 1');
  });
  // NEGATIVE PROOF (#3734): the same run with no reasons in the store and no
  // errors line shows none of it, so the text above comes from the rows and
  // the run line, not from the renderer.
  it('without the reasons and the errors line, none of that text is on the page', () => {
    const bare = renderNightlyPage(parseNightlyLog(RUN_NO_ERRORS), { cases: parseFailingCases(csvOf(false)) });
    expect(bare).not.toContain('Cannot read properties');
    expect(bare).not.toContain('Received: 500');
    expect(bare).not.toContain(SHELL_503);
    expect(bare).toContain('errors</span> unmeasured — the run wrote no errors line');
  });
});

describe('#4155 the readout Jeff receives counts errors', () => {
  it('names the counts from the run own line', () => {
    const r = buildReadout(parseAllRuns(RUN)[0], null);
    expect(r.errors).toBe('failed cases 3 · exceptions 1 · http 1 · assertions 0 · other 1');
    expect(renderReadoutText(r, 'http://x').split('\n')[1])
      .toBe('errors: failed cases 3 · exceptions 1 · http 1 · assertions 0 · other 1');
  });
  // NEGATIVE PROOF: an old run with no errors line never reads as zero errors
  it('a run with no errors line says not measured, never 0', () => {
    const r = buildReadout(parseAllRuns(RUN_NO_ERRORS)[0], null);
    expect(r.errors).toBeNull();
    const text = renderReadoutText(r, 'http://x');
    expect(text).toContain('errors: not measured');
    expect(text).not.toContain('exceptions 0');
  });
});
