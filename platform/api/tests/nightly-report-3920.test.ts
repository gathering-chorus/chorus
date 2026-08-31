// @test-type: unit
// #3920 — /nightly: the rendered viewing surface for the nightly run.
// Jeff's bar: he opens ONE page and sees whether the night was green — never
// a list of CLI steps. Hermetic: the parser/renderer are pure; the fixture is
// the log format nightly-suites.sh actually writes (RUN|/SUITE| lines).
import { parseNightlyLog, renderNightlyPage } from '../src/handlers/nightly-report';

const GREEN_RUN = [
  'RUN|start|2026-08-21T03:00:01',
  'SUITE|lint|/chorus|kade|pass|1 pass, 0 fail (lint:ratchet clean)',
  'SUITE|cargo|platform/services/werk-test|silas|pass|135 pass, 0 fail',
  'SUITE|npm|/chorus/platform/api|silas|pass|Tests: 200 passed',
  'RUN|complete|2026-08-21T03:41:09',
].join('\n');

const RED_RUN = [
  'RUN|start|2026-08-22T03:00:01',
  'SUITE|cargo|platform/services/chorus-hooks|silas|fail|30 pass, 2 fail',
  'SUITE|cargo|platform/services/werk-test|silas|pass|135 pass, 0 fail',
  'SUITE|shell|/x/test-api-health.sh|silas|skip|skipped — no live stack (#3557)',
  'RUN|complete|2026-08-22T03:39:12',
].join('\n');

describe('parseNightlyLog', () => {
  it('parses the LAST run block with its rows and completion', () => {
    const run = parseNightlyLog(GREEN_RUN + '\n' + RED_RUN);
    expect(run).not.toBeNull();
    expect(run!.startedAt).toBe('2026-08-22T03:00:01');
    expect(run!.completed).toBe(true);
    expect(run!.rows).toHaveLength(3);
    expect(run!.rows[0]).toEqual({
      kind: 'cargo',
      path: 'platform/services/chorus-hooks',
      owner: 'silas',
      status: 'fail',
      summary: '30 pass, 2 fail',
    });
  });

  it('a killed run (start without complete) is named partial, never green', () => {
    const run = parseNightlyLog('RUN|start|2026-08-22T03:00:01\nSUITE|cargo|x|silas|pass|1 pass, 0 fail');
    expect(run!.completed).toBe(false);
  });

  it('no runs at all is null (page renders the empty state, not a fake green)', () => {
    expect(parseNightlyLog('')).toBeNull();
    expect(parseNightlyLog('unrelated noise\n')).toBeNull();
  });
});

describe('renderNightlyPage', () => {
  it('a red night leads with the red verdict and the failing suite', () => {
    const html = renderNightlyPage(parseNightlyLog(RED_RUN));
    // NEGATIVE PROOF (#3734): the red state must be visibly distinct — the
    // verdict word, the count, and the failing row all present.
    expect(html).toContain('1 RED');
    expect(html).toContain('chorus-hooks');
    expect(html).toContain('30 pass, 2 fail');
    expect(html).not.toContain('ALL GREEN');
  });

  it('a green night says so, with the zero-red bar met', () => {
    const html = renderNightlyPage(parseNightlyLog(GREEN_RUN));
    expect(html).toContain('ALL GREEN');
    expect(html).not.toContain('1 RED');
  });

  it('typed skips render as skips, counted, never as pass or fail', () => {
    const html = renderNightlyPage(parseNightlyLog(RED_RUN));
    expect(html).toMatch(/1 skipped/i);
  });

  it('an incomplete run renders the loud partial banner', () => {
    const html = renderNightlyPage(
      parseNightlyLog('RUN|start|2026-08-22T03:00:01\nSUITE|cargo|x|silas|pass|1 pass, 0 fail')
    );
    expect(html).toMatch(/PARTIAL|never completed/i);
  });

  it('no run yet renders an honest empty state', () => {
    const html = renderNightlyPage(null);
    expect(html).toMatch(/no nightly run/i);
  });
});

describe('displayPath (#3964 — readable on a phone)', () => {
  const abs = '/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/test-api-health.sh';
  const RUN = [
    'RUN|start|2026-08-22T03:00:01',
    `SUITE|shell|${abs}|silas|fail|5 pass, 2 fail`,
    'SUITE|cargo|platform/services/werk-test|silas|pass|135 pass, 0 fail',
    'RUN|complete|2026-08-22T03:39:12',
  ].join('\n');

  it('absolute suite paths render repo-relative; relative ones unchanged', () => {
    const html = renderNightlyPage(parseNightlyLog(RUN));
    // NEGATIVE PROOF (#3734): the violation — the raw /Users prefix on screen —
    // must be absent; the readable form present.
    expect(html).not.toContain('/Users/jeffbridwell');
    expect(html).toContain('platform/scripts/test-api-health.sh');
    expect(html).toContain('platform/services/werk-test');
  });

  it('app-root paths shorten too', () => {
    const run = parseNightlyLog([
      'RUN|start|2026-08-22T03:00:01',
      'SUITE|npm|/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site|kade|pass|Tests: 9 passed',
      'RUN|complete|2026-08-22T03:39:12',
    ].join('\n'));
    const html = renderNightlyPage(run);
    expect(html).not.toContain('/Users/jeffbridwell');
    expect(html).toContain('app:jeff-bridwell-personal-site');
  });
});

// #4030 AC4 — /nightly: a planned-but-unreached suite is a red row with its
// reason, so "was the night green?" cannot answer yes over units that never ran.
describe('#4030 never-ran suites render red on /nightly', () => {
  const log = [
    'RUN|start|2026-08-30T03:00:00',
    'SUITE|cargo|platform/services/werk-test|silas|pass|191 pass, 0 fail',
    'SUITE|npm|platform/api|silas|fail|0 pass, 1 fail (NEVER RAN — the runner was killed at the lane cap before this unit (rc=124))',
    'RUN|complete|2026-08-30T05:15:54',
  ].join('\n');

  it('counts the unreached unit as RED and shows why', () => {
    const run = parseNightlyLog(log)!;
    const html = renderNightlyPage(run);
    expect(html).toContain('1 RED');
    expect(html).toContain('NEVER RAN');
    expect(html).not.toContain('ALL GREEN');
  });

  it('control: without the row the night is ALL GREEN', () => {
    const run = parseNightlyLog(log.split('\n').filter(l => !l.includes('NEVER RAN')).join('\n'))!;
    expect(renderNightlyPage(run)).toContain('ALL GREEN');
  });
});

// #4035 — a STOPPED run names itself. The wrapper's TERM handler writes
// RUN|stopped; the page must say STOPPED (who-ended-it), never the wedge
// banner ("treat it as wedged") and never a blank.
describe('#4035 stopped runs say so', () => {
  const log = [
    'RUN|start|2026-08-31T03:00:01',
    'SUITE|cargo|platform/services/werk-test|silas|pass|191 pass, 0 fail',
    'RUN|stopped|2026-08-31T03:21:44|signal=TERM pid=17545',
  ].join('\n');

  it('negative proof: the stop marker renders a STOPPED banner, not wedged/blank', () => {
    const run = parseNightlyLog(log)!;
    expect(run.completed).toBe(false);
    expect(run.stoppedAt).toBe('2026-08-31T03:21:44');
    const html = renderNightlyPage({ ...run, quietForMs: 60 * 60 * 1000 });
    expect(html).toContain('STOPPED at 2026-08-31T03:21:44');
    expect(html).toContain('signal=TERM');
    expect(html).not.toContain('Treat it as wedged');
  });

  it('control: without the marker the same quiet run still reads as wedged', () => {
    const run = parseNightlyLog(log.split('\n').slice(0, 2).join('\n'))!;
    const html = renderNightlyPage({ ...run, quietForMs: 60 * 60 * 1000 });
    expect(html).toContain('wedged');
    expect(html).not.toContain('STOPPED at');
  });

  it('a stopped block does not swallow rows from the next run', () => {
    const two = log + '\n' + 'RUN|start|2026-08-31T09:00:00\nSUITE|lint|/x|kade|pass|1 pass, 0 fail';
    const run = parseNightlyLog(two)!;
    expect(run.startedAt).toBe('2026-08-31T09:00:00');
    expect(run.rows).toHaveLength(1);
  });
});
