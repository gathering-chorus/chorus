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
    expect(r.reds).toEqual([
      { owner: 'silas', suite: 'platform/tests/vocab-claim-authority.bats', kind: 'bats' },
      { owner: 'silas', suite: 'platform/api', kind: 'npm' },
      { owner: 'kade', suite: 'tests-domain', kind: 'cargo' },
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
