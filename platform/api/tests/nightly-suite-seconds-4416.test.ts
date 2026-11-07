// @test-type: unit
// @domain: tests · @card: 4416 · owner: kade
// #4416 — Jeff, 2026-10-01: "we have run times for suites now?" The page lists
// the run's slowest suites from the seconds the run log carries (SUITETIME, #4253);
// #4579 reopen moved the page off TestSuiteRun rows onto the log.
import { parseAllRuns } from '../src/handlers/nightly-readout';
import { renderNightlyPage, slowestSuites } from '../src/handlers/nightly-report';

function run(secs: (string | undefined)[]) {
  const lines = ['RUN|start|2026-10-01T16:30:00|pid=1'];
  secs.forEach((s, i) => {
    lines.push(`SUITE|bats|platform/tests/s${i + 1}.bats|kade|pass|1 pass, 0 fail`);
    if (s !== undefined) lines.push(`SUITETIME|bats|platform/tests/s${i + 1}.bats|${s}`);
  });
  lines.push('RUN|complete|2026-10-01T18:00:00|suites=' + secs.length);
  return parseAllRuns(lines.join('\n'))[0];
}

describe('#4416 the slowest suites, from the seconds the run wrote', () => {
  it('orders the timed suites slowest first and keeps at most ten', () => {
    const r = run(['3.0', '600.5', '12.0', ...Array(10).fill('1.0')]);
    const top = slowestSuites(r.rows);
    expect(top).toHaveLength(10);
    expect(top.slice(0, 3).map((x) => [x.path, x.seconds])).toEqual([
      ['platform/tests/s2.bats', 600.5], ['platform/tests/s3.bats', 12], ['platform/tests/s1.bats', 3],
    ]);
  });

  it('the page shows the fold with each suite and its seconds', () => {
    const html = renderNightlyPage(run(['3.0', '600.5']));
    expect(html).toContain('2 slowest suites');
    // #4253 — every suite row now shows its seconds too, so read the order
    // inside the slowest fold only
    const fold = html.slice(html.indexOf('slowest suites'));
    expect(fold.indexOf('600.5 s')).toBeLessThan(fold.indexOf('3.0 s'));
    expect(html).toContain('s2.bats');
  });

  it('NEGATIVE PROOF: a run whose rows carry no seconds shows no slowest fold, never a guess', () => {
    const r = run([undefined, undefined]);
    expect(r.rows.every((x) => x.seconds === undefined)).toBe(true);
    expect(renderNightlyPage(r)).not.toContain('slowest suites');
  });

  it('a row that is not a number is left untimed', () => {
    expect(run(['abc'])).toMatchObject({ rows: [{ path: 'platform/tests/s1.bats' }] });
    expect(run(['abc']).rows[0].seconds).toBeUndefined();
  });
});
