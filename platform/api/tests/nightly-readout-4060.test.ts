// @test-type: unit — signal:api is fixture-data (pure functions over log fixtures; the word "api" is a suite path in them)
// #4060 — "I don't even get a readout on the run."
//
// One readout per run, in Jeff's units (minutes, suites, who owns each red,
// what changed since the last run), computed ONCE from the run record and
// served to everyone — the nudge, the page, and any role asked — from the same
// function. Two roles asked about the same run give the same numbers because
// there is only one place the numbers come from.
//
// Hermetic: fixtures are the log format nightly-suites.sh writes.
import {
  parseAllRuns, findRun, buildReadout, renderReadoutText,
} from '../src/handlers/nightly-readout';
import { renderNightlyPage } from '../src/handlers/nightly-report';

const RUN_A = [
  'RUN|start|2026-09-01T03:00:05|pid=1',
  'SUITE|cargo|platform/services/chorus-hooks|silas|fail|30 pass, 2 fail',
  'SUITE|bats|platform/tests/vocab-claim-authority.bats|silas|fail|bats: 3 passed, 1 failed',
  'SUITE|npm|/Users/j/CascadeProjects/chorus/platform/api|silas|pass|Tests: 200 passed',
  'SUITE|shell|platform/scripts/test-role-state-spine.sh|silas|pass|4 pass, 0 fail',
  'SUITE|bats|platform/tests/old-suite.bats|kade|pass|bats: 1 passed, 0 failed',
  'RUN|complete|2026-09-01T04:11:05|suites=5',
].join('\n');

const RUN_B = [
  'RUN|start|2026-09-02T03:00:05|pid=2',
  'SUITE|cargo|platform/services/chorus-hooks|silas|pass|32 pass, 0 fail',
  'SUITE|bats|platform/tests/vocab-claim-authority.bats|silas|fail|bats: 3 passed, 1 failed',
  'SUITE|npm|/Users/j/CascadeProjects/chorus/platform/api|silas|fail|Tests: 2 failed, 198 passed',
  'SUITE|shell|platform/scripts/test-role-state-spine.sh|silas|skip|skipped — no live stack',
  'SUITE|cargo|tests-domain|kade|fail|0 pass, 1 fail',
  'RUN|complete|2026-09-02T03:47:35|suites=5',
].join('\n');

const LOG = RUN_A + '\n' + RUN_B + '\n';

describe('parseAllRuns — history, not only the newest', () => {
  it('returns every run block in order, each identified by its start time', () => {
    const runs = parseAllRuns(LOG);
    expect(runs.map((r) => r.runId)).toEqual(['2026-09-01T03:00:05', '2026-09-02T03:00:05']);
    expect(runs[0].rows).toHaveLength(5);
    expect(runs[1].completed).toBe(true);
  });

  it('findRun resolves "latest" and any past id; an unknown id is null, never the newest', () => {
    const runs = parseAllRuns(LOG);
    expect(findRun(runs, 'latest')!.runId).toBe('2026-09-02T03:00:05');
    expect(findRun(runs, '2026-09-01T03:00:05')!.runId).toBe('2026-09-01T03:00:05');
    // NEGATIVE PROOF (#3734): asking for a run that never happened must not
    // quietly answer with a different run's numbers.
    expect(findRun(runs, '2026-08-30T03:00:05')).toBeNull();
  });
});

describe('buildReadout — Jeff\'s units', () => {
  const runs = parseAllRuns(LOG);
  const r = buildReadout(runs[1], runs[0]);

  it('says how long it took, in minutes', () => {
    expect(r.durationMin).toBe(48); // 03:00:05 → 03:47:35 = 47.5 → rounds up
  });

  it('counts suites ran, and which are red, with an owner on each', () => {
    expect(r.suites).toBe(5);
    expect(r.failed).toBe(3);
    expect(r.passed).toBe(1);
    expect(r.skipped).toBe(1);
    // #4073 — each red now carries its derived label; with two runs of history
    // and no flips, all three read as product-broke (see nightly-red-labels-4073)
    expect(r.reds).toEqual([
      { owner: 'silas', suite: 'platform/tests/vocab-claim-authority.bats', kind: 'bats', label: 'product-broke' },
      { owner: 'silas', suite: 'platform/api', kind: 'npm', label: 'product-broke' },
      { owner: 'kade', suite: 'tests-domain', kind: 'cargo', label: 'product-broke' },
    ]);
    expect(r.redByOwner).toEqual({ silas: 2, kade: 1 });
  });

  it('says what changed since the last run: new reds, fixed, and gone', () => {
    expect(r.changes.previousRunId).toBe('2026-09-01T03:00:05');
    expect(r.changes.newlyRed.map((x) => x.suite)).toEqual(['platform/api', 'tests-domain']);
    expect(r.changes.fixed.map((x) => x.suite)).toEqual(['platform/services/chorus-hooks']);
    expect(r.changes.stillRed.map((x) => x.suite)).toEqual(['platform/tests/vocab-claim-authority.bats']);
    expect(r.changes.gone).toEqual(['platform/tests/old-suite.bats']);
  });

  it('with no previous run, changes are unknown — not "0 new, 0 fixed"', () => {
    const first = buildReadout(runs[0], null);
    // NEGATIVE PROOF (#3734): an absence of history must not read as "nothing changed".
    expect(first.changes.previousRunId).toBeNull();
    expect(first.changes.newlyRed).toEqual([]);
    expect(renderReadoutText(first, 'http://x')).toMatch(/no earlier run to compare/i);
  });

  it('a run that never completed is named partial in the readout, never a full night', () => {
    const partial = parseAllRuns('RUN|start|2026-09-03T03:00:00|pid=9\nSUITE|cargo|a|silas|pass|1 pass, 0 fail\n');
    const p = buildReadout(partial[0], null);
    expect(p.completed).toBe(false);
    expect(p.durationMin).toBeNull();
    expect(renderReadoutText(p, 'http://x')).toMatch(/PARTIAL/);
  });
});

describe('renderReadoutText — the message Jeff receives', () => {
  const runs = parseAllRuns(LOG);
  const text = renderReadoutText(buildReadout(runs[1], runs[0]), 'http://localhost:3340');

  it('carries every number the JSON does, in the same units', () => {
    expect(text).toContain('48 min');
    expect(text).toContain('5 suites');
    expect(text).toContain('3 red');
    expect(text).toContain('silas 2');
    expect(text).toContain('kade 1');
    expect(text).toContain('2 new red');
    expect(text).toContain('1 fixed');
    expect(text).toContain('1 still red');
  });

  it('names each red with its owner and links the run by its own id', () => {
    expect(text).toContain('silas  platform/api');
    expect(text).toContain('kade   tests-domain');
    expect(text).toContain('http://localhost:3340/nightly?run=2026-09-02T03:00:05');
  });

  it('a green run says green and links the same way', () => {
    const green = parseAllRuns('RUN|start|2026-09-04T03:00:00|pid=1\nSUITE|cargo|a|silas|pass|1 pass, 0 fail\nRUN|complete|2026-09-04T03:30:00|suites=1\n');
    const t = renderReadoutText(buildReadout(green[0], null), 'http://x');
    expect(t).toContain('0 red');
    expect(t).toContain('all green');
  });
});

describe('/nightly page — any past run, with the readout on top', () => {
  const runs = parseAllRuns(LOG);

  it('renders a past run by id with a history list linking every run', () => {
    const html = renderNightlyPage(runs[0], {
      readout: buildReadout(runs[0], null), history: runs,
    });
    expect(html).toContain('2026-09-01T03:00:05');
    expect(html).toContain('href="/nightly?run=2026-09-02T03:00:05"');
    expect(html).toContain('2 RED');
    // the past run's own reds, not the newest run's
    expect(html).toContain('chorus-hooks');
    expect(html).not.toContain('tests-domain');
  });

  it('the readout banner shows duration and the delta', () => {
    const html = renderNightlyPage(runs[1], {
      readout: buildReadout(runs[1], runs[0]), history: runs,
    });
    expect(html).toContain('48 min');
    expect(html).toContain('2 new red');
    expect(html).toContain('1 fixed');
  });
});

describe('/nightly page — a partial run has no verdict', () => {
  it('NEGATIVE PROOF (#4063): 13 suites in and 0 red so far must NOT banner ALL GREEN', () => {
    const partial = parseAllRuns('RUN|start|2026-09-02T13:30:00|pid=1\n' +
      Array.from({ length: 13 }, (_, i) => `SUITE|cargo|c${i}|silas|pass|1 pass, 0 fail`).join('\n') + '\n');
    const html = renderNightlyPage(partial[0], { readout: buildReadout(partial[0], null), history: partial });
    expect(html).not.toContain('ALL GREEN');
    expect(html).toContain('IN PROGRESS');
    expect(html).toContain('13 suite(s) so far');
    expect(html).toContain('0 red so far');
  });

  it('a completed all-green run still says ALL GREEN — the fix must not widen into never-green', () => {
    const done = parseAllRuns('RUN|start|2026-09-02T13:30:00|pid=1\nSUITE|cargo|c|silas|pass|1 pass, 0 fail\nRUN|complete|2026-09-02T14:30:00|suites=1\n');
    expect(renderNightlyPage(done[0])).toContain('ALL GREEN');
  });
});

// #4271 — the readout stated ONE grain. Jeff, 2026-09-22: the graph said
// "17 failed of 421" and the log said "51 failed of 9,417" for the same run,
// and nothing said which unit either was counting. The readout only ever knew
// the suite grain: buildReadout computes from the SUITE rows, and the run's
// own RUN|tally line was dropped by the parser before it ever got there.
//
// The tally comes through VERBATIM. The readout never recomputes a test
// number — a third computation of the test grain is the defect again.
const RUN_WITH_TALLY = [
  'RUN|start|2026-09-22T03:00:03|pid=10432',
  'SUITE|bats|platform/tests/a.bats|kade|pass|2 pass, 0 fail',
  'SUITE|bats|platform/tests/b.bats|wren|fail|0 pass, 1 fail',
  'RUN|tally|registered 8749 · ran 9417 · passed 9320 · failed 51 · unmeasured 46 · no result 40',
  'RUN|complete|2026-09-22T03:49:08|suites=2',
].join('\n');

const RUN_NO_TALLY = [
  'RUN|start|2026-09-21T03:00:03|pid=1',
  'SUITE|bats|platform/tests/a.bats|kade|pass|2 pass, 0 fail',
  'RUN|complete|2026-09-21T03:40:03|suites=1',
].join('\n');

describe('#4271 — the readout carries both grains, each labelled', () => {
  it('reads the run\'s own tally and states tests beside suites', () => {
    const runs = parseAllRuns(RUN_WITH_TALLY);
    const r = buildReadout(runs[0], null, runs);
    expect(r.tests).not.toBeNull();
    expect(r.tests).toMatchObject({ registered: 8749, ran: 9417, passed: 9320, failed: 51, noResult: 40 });
    const text = renderReadoutText(r, 'http://x');
    expect(text).toContain('2 suites');
    expect(text).toContain('9,417 tests');
    expect(text).toContain('51 red');
  });

  it('takes the numbers VERBATIM from the tally — it never recomputes them', () => {
    // a tally that disagrees with the suite rows is still reported as written:
    // the readout's job is to state the run's record, not to audit it
    const odd = RUN_WITH_TALLY.replace('failed 51', 'failed 7');
    const runs = parseAllRuns(odd);
    const r = buildReadout(runs[0], null, runs);
    expect(r.tests?.failed).toBe(7);
    expect(r.failed).toBe(1); // the suite grain is untouched by it
  });

  // NEGATIVE PROOF (#3734): absent is not zero. A run that measured no tests
  // must say so. "0 tests failed" on an unmeasured night reads as green, which
  // is the whole class of defect this card exists to close.
  it('NEGATIVE PROOF: a run with no tally reports the test grain ABSENT, never 0', () => {
    const runs = parseAllRuns(RUN_NO_TALLY);
    const r = buildReadout(runs[0], null, runs);
    expect(r.tests).toBeNull();
    const text = renderReadoutText(r, 'http://x');
    expect(text).toContain('tests not measured');
    expect(text).not.toMatch(/\b0 tests\b/);
  });

  // And a tally the runner could not compute is absent too, not zero.
  it('NEGATIVE PROOF: an unreadable registry reports absent, not a row of zeroes', () => {
    const unreadable = RUN_WITH_TALLY.replace(
      /RUN\|tally\|.*/,
      'RUN|tally|registry unreadable — the run cannot say what it did not run',
    );
    const r = buildReadout(parseAllRuns(unreadable)[0], null, []);
    expect(r.tests).toBeNull();
    expect(renderReadoutText(r, 'http://x')).toContain('tests not measured');
  });
});
