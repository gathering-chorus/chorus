// @test-type: unit — signal:fs is a temp log file the test writes, no live service
// @domain: tests — picked by its owner, Kade, from the crawler's candidates (#4538)
// #4007 — Jeff opens one page and sees which suite is running and how long it
// has been running, and a hung suite reads WEDGED without anyone running ps.
// #4579 reopen — read from the run log's SUITESTART/SUITEEND lines (the start
// carries the unit's timeout), not the graph's live rows. Hermetic: a temp log.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { loadRunsFromLog } from '../src/handlers/nightly-log';
import { inFlightLine, isWedged, renderNightlyPage } from '../src/handlers/nightly-report';

const NOW = Date.parse('2026-10-10T03:30:00Z');
const RUN = '2026-10-10T03:00:03';

/** A log whose newest run has these units started `ago` ms before NOW, and not ended. */
function logWith(units: [string, number][], done = false): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nightly-4007-'));
  const f = path.join(dir, 'nightly-suites.log');
  const lines = [`RUN|start|${RUN}|pid=1`, 'SUITE|bats|platform/tests/a.bats|kade|pass|1 pass'];
  for (const [u, ago] of units) lines.push(`SUITESTART|${u}|${NOW - ago}|timeout=1200s`);
  if (done) lines.push('RUN|complete|2026-10-10T05:00:00|suites=1');
  fs.writeFileSync(f, lines.join('\n') + '\n');
  return f;
}

describe('#4007 the page names the suites running now', () => {
  it('a live run shows each running suite with its time, longest first', async () => {
    const runs = await loadRunsFromLog(logWith([['platform/tests/b.bats', 125_000], ['werk-test#units', 30_000]]), 14, NOW);
    const run = runs![runs!.length - 1];
    expect(run.inFlight).toHaveLength(2);
    const line = inFlightLine(run);
    expect(line).toContain('Running now: platform/tests/b.bats 2m 5s · werk-test#units 30s');
    expect(line).not.toContain('WEDGED');
    expect(renderNightlyPage(run)).toContain('Running now:');
  });

  it('NEGATIVE PROOF: a suite hung past its own timeout reads WEDGED; the healthy one beside it does not', async () => {
    const runs = await loadRunsFromLog(logWith([['platform/tests/hung.bats', 21 * 60_000], ['platform/tests/ok.bats', 5 * 60_000]]), 14, NOW);
    const run = runs![runs!.length - 1];
    const line = inFlightLine(run);
    expect(line).toContain('<b>WEDGED</b> platform/tests/hung.bats 21m 0s (past its 20m 0s timeout)');
    expect(line).not.toMatch(/WEDGED<\/b> platform\/tests\/ok\.bats/);
    expect(line.indexOf('hung.bats')).toBeLessThan(line.indexOf('ok.bats'));
  });

  it('a suite one second under its timeout is not wedged; at the timeout it is', () => {
    expect(isWedged({ unit: 'x', elapsedMs: 1_199_000, timeoutMs: 1_200_000 })).toBe(false);
    expect(isWedged({ unit: 'x', elapsedMs: 1_200_000, timeoutMs: 1_200_000 })).toBe(true);
    expect(isWedged({ unit: 'x', elapsedMs: 99_000_000 })).toBe(false);
  });

  it('a finished run shows no running line, even with a unit never ended', async () => {
    const runs = await loadRunsFromLog(logWith([['platform/tests/stale.bats', 60_000]], true), 14, NOW);
    const run = runs![runs!.length - 1];
    expect(run.completed).toBe(true);
    expect(run.inFlight).toBeUndefined();
    expect(inFlightLine(run)).toBe('');
  });
});
