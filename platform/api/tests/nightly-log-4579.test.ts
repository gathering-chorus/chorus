// @test-type: unit — signal:fs is a temp log file the test writes, no live service
// @domain: tests — Kade (#4579 reopen)
// #4579 reopen — Jeff 10-18 19:10 "yes": /nightly reads the run log, and the
// graph stops carrying TestSuiteRun. Measured 19:12: the graph had the 10-18
// 03:00 run as not finished at 287 suites, while the log ended it at 06:08:56
// with suites=504. The log is the record the runner writes in one place.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { loadRunsFromLog, inFlightFromLog, tailRuns } from '../src/handlers/nightly-log';
import { isWedged } from '../src/handlers/nightly-report';

const RUN1 = [
  'RUN|start|2026-10-17T03:00:03|pid=1',
  'SUITESTART|platform/tests/a.bats|1790000000000',
  'SUITE|bats|platform/tests/a.bats|kade|pass|bats: 3 passed',
  'SUITEEND|platform/tests/a.bats|1790000001000',
  'RUN|tally|registered 3 · ran 3 · passed 3 · failed 0 · unmeasured 0 · no result 0',
  'RUN|errors|failed cases 0 · exceptions 0 · http 0 · assertions 0 · other 0',
  'RUN|complete|2026-10-17T05:19:35|suites=1',
];
const RUN2_LIVE = [
  'RUN|start|2026-10-18T03:00:03|pid=2',
  'SUITESTART|platform/tests/a.bats|1790000100000',
  'SUITE|bats|platform/tests/a.bats|kade|fail|bats: 2 passed, 1 failed',
  'SUITEEND|platform/tests/a.bats|1790000101000',
  'SUITESTART|platform/services/werk-test|1790000102000',
];

function logFile(lines: string[]): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nightly-log-4579-'));
  const f = path.join(dir, 'nightly-suites.log');
  fs.writeFileSync(f, lines.join('\n') + '\n');
  return f;
}

describe('#4579 /nightly reads the run log', () => {
  it('a finished run comes back finished, with its rows, tally and errors', async () => {
    const runs = await loadRunsFromLog(logFile(RUN1), 14, 1790000200000);
    expect(runs).not.toBeNull();
    expect(runs!.map((r) => r.runId)).toEqual(['2026-10-17T03:00:03']);
    expect(runs![0].completed).toBe(true);
    expect(runs![0].completedAt).toBe('2026-10-17T05:19:35');
    expect(runs![0].rows).toHaveLength(1);
    expect(runs![0].tally).toBeDefined();
    expect(runs![0].errors).toContain('failed cases 0');
  });

  it('the newest unfinished run names the suite still running and how quiet it is', async () => {
    const f = logFile([...RUN1, ...RUN2_LIVE]);
    const mtime = fs.statSync(f).mtimeMs;
    const runs = await loadRunsFromLog(f, 14, mtime + 5000);
    const newest = runs![runs!.length - 1];
    expect(newest.completed).toBe(false);
    expect(newest.inFlight).toEqual([{ unit: 'platform/services/werk-test', elapsedMs: mtime + 5000 - 1790000102000 }]);
    expect(newest.quietForMs).toBe(5000);
    // NEGATIVE PROOF: a finished run never carries in-flight units or quiet time
    expect(runs![0].inFlight).toBeUndefined();
    expect(runs![0].quietForMs).toBeUndefined();
  });

  it('a unit that ended is not in flight', () => {
    expect(inFlightFromLog(RUN1, 1790000200000)).toEqual([]);
    // NEGATIVE PROOF: drop the SUITEEND and the unit is in flight
    expect(inFlightFromLog(RUN1.filter((l) => !l.startsWith('SUITEEND')), 1790000200000))
      .toEqual([{ unit: 'platform/tests/a.bats', elapsedMs: 200000 }]);
  });

  it('a unit past the timeout its start line carries reads WEDGED (#4007)', () => {
    const hung = inFlightFromLog(['SUITESTART|platform/tests/h.bats|1000|timeout=60s'], 1000 + 61_000);
    expect(hung).toEqual([{ unit: 'platform/tests/h.bats', elapsedMs: 61_000, timeoutMs: 60_000 }]);
    expect(isWedged(hung[0])).toBe(true);
    // NEGATIVE PROOF: under its timeout it is not wedged
    expect(isWedged(inFlightFromLog(['SUITESTART|platform/tests/h.bats|1000|timeout=60s'], 1000 + 30_000)[0])).toBe(false);
  });

  it('only the last `limit` runs are read, oldest first', () => {
    const many: string[] = [];
    for (let d = 1; d <= 20; d++) many.push(...RUN1.map((l) => l.replace('2026-10-17T03', `2026-09-${String(d).padStart(2, '0')}T03`)));
    const runs = tailRuns(many.join('\n'), 14);
    expect(runs).toHaveLength(14);
    expect(runs[0].runId).toBe('2026-09-07T03:00:03');
    expect(runs[13].runId).toBe('2026-09-20T03:00:03');
  });

  it('a missing log is null (the route refuses), never an empty green', async () => {
    expect(await loadRunsFromLog('/nonexistent/nightly-suites.log', 14)).toBeNull();
  });
});
