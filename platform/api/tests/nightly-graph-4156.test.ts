// @test-type: unit
// #4156 — the nightly page and the morning readout render from the graph rows
// the run wrote, not from the log. Jeff's spec line (2026-09-12): "summary
// reporting provided from logs and/or graph".
//
// Hermetic: the same run expressed two ways (the log-format fixture, and the
// TestSuiteRun + PipelineRun rows the runner now writes for it) must render the
// same page and the same readout. The graph is the only source the routes use.
import { runsFromGraph, loadRunsFromGraph, csvRecords, type CsvQuery } from '../src/handlers/nightly-graph';
import { parseAllRuns, buildReadout, renderReadoutText } from '../src/handlers/nightly-readout';
import { renderNightlyPage } from '../src/handlers/nightly-report';
import { graphFromLog, SUITE_HEAD, RECORD_HEAD } from './lib/nightly-graph-fixture';

const LOG = [
  'RUN|start|2026-09-24T03:00:00|pid=1',
  'SUITE|lint|/chorus|kade|pass|1 pass, 0 fail (lint:ratchet clean)',
  'SUITE|cargo|platform/services/chorus-principal|silas|fail|40 pass, 1 fail',
  'SUITE|bats|platform/tests/a.bats|principal-kade|skip|0 pass, 0 fail, 6 skipped (ALL SKIPPED — not built)',
  'RUN|tally|registered 9530 · ran 9508 · passed 9480 · failed 1 · unmeasured 27 · no result 22',
  'RUN|errors|failed cases 1 · exceptions 0 · http 1 · assertions 0 · other 0',
  'RUN|complete|2026-09-24T04:01:00|suites=3',
  'RUN|start|2026-09-25T15:37:59|pid=2',
  'SUITE|lint|/chorus|kade|pass|1 pass, 0 fail (lint:ratchet clean)',
  'SUITE|cargo|platform/services/chorus-principal|silas|pass|41 pass, 0 fail',
  'SUITE|bats|platform/tests/a.bats|principal-kade|fail|bats: 5 passed, 1 failed',
  'RUN|tally|registered 9530 · ran 9510 · passed 9500 · failed 1 · unmeasured 9 · no result 20',
  'RUN|errors|failed cases 1 · exceptions 1 · http 0 · assertions 0 · other 0',
  'RUN|complete|2026-09-25T16:40:01|suites=3',
  '',
].join('\n');

const fromLog = parseAllRuns(LOG);
const g = graphFromLog(LOG);
const fromGraph = runsFromGraph(g.suites, g.records, 1790000000000);

describe('#4156 the graph rows make the same run the log did', () => {
  it('same runs, same rows in the same order, same tally and errors', () => {
    expect(fromGraph.map((r) => r.runId)).toEqual(fromLog.map((r) => r.runId));
    fromGraph.forEach((r, i) => {
      expect(r.rows).toEqual(fromLog[i].rows);
      expect(r.completed).toBe(true);
      expect(r.completedAt).toBe(fromLog[i].completedAt);
      expect(r.tally).toEqual(fromLog[i].tally);
      expect(r.errors).toBe(fromLog[i].errors);
    });
  });
  it('the readout Jeff receives is word for word the same', () => {
    const text = (runs: typeof fromLog) => renderReadoutText(buildReadout(runs[1], runs[0], runs), 'http://x');
    expect(text(fromGraph)).toBe(text(fromLog));
    expect(text(fromGraph)).toContain('errors: failed cases 1 · exceptions 1');
  });
  it('the page is the same page', () => {
    const html = (runs: typeof fromLog) => renderNightlyPage(runs[1], { readout: buildReadout(runs[1], runs[0], runs), history: runs, cases: {} });
    expect(html(fromGraph)).toBe(html(fromLog));
  });
  it('rows are placed by suiteOrder, not by the order the store returns them', () => {
    const lines = g.suites.trim().split('\n');
    const shuffled = [lines[0], ...lines.slice(1).reverse()].join('\n');
    expect(runsFromGraph(shuffled, g.records, 0)[1].rows).toEqual(fromLog[1].rows);
  });
});

describe('#4156 a run still going, or stopped, reads as such from the graph', () => {
  const live = [SUITE_HEAD, '2026-09-25T15:37:59,1,lint,/chorus,kade,pass,"1 pass, 0 fail",1790365080000'].join('\n');
  it('suite rows with no run record: not complete, quiet time from the newest row', () => {
    const [run] = runsFromGraph(live, RECORD_HEAD + '\n', 1790365080000 + 120000);
    expect(run.completed).toBe(false);
    expect(run.quietForMs).toBe(120000);
    expect(renderNightlyPage(run)).toMatch(/still running|RUNNING|running/i);
  });
  it('a record with outcome stopped: the page says stopped, not complete', () => {
    const rec = [RECORD_HEAD, '2026-09-25T15:37:59,stopped,2026-09-25T15:50:00,,,,,,,,,,,'].join('\n');
    const [run] = runsFromGraph(live, rec, 0);
    expect(run.completed).toBe(false);
    expect(run.stoppedAt).toBe('2026-09-25T15:50:00');
  });
  it('a record with no tally leaves the test grain unmeasured, never zero', () => {
    const rec = [RECORD_HEAD, '2026-09-25T15:37:59,green,2026-09-25T16:00:00,,,,,,,,,,,'].join('\n');
    const [run] = runsFromGraph(live, rec, 0);
    expect(run.tally).toBeUndefined();
    expect(run.errors).toBeUndefined();
    expect(renderReadoutText(buildReadout(run, null), 'http://x')).toContain('tests not measured');
  });
});

describe('#4156 the store is the only source', () => {
  const store = (answers: Record<string, string | null>): CsvQuery => async (q) => {
    if (q.includes('SELECT DISTINCT ?runTs')) return answers.ids ?? null;
    if (q.includes('c:TestSuiteRun')) return answers.suites ?? null;
    return answers.records ?? null;
  };
  it('loads the runs the store holds', async () => {
    const ids = 'runTs\n2026-09-25T15:37:59\n2026-09-24T03:00:00\n';
    const runs = await loadRunsFromGraph(store({ ids, suites: g.suites, records: g.records }));
    expect(runs!.map((r) => r.runId)).toEqual(['2026-09-24T03:00:00', '2026-09-25T15:37:59']);
  });
  // NEGATIVE PROOF — the run's rows deleted: the loader returns no run and the
  // page says so in words; it does not fall back to anything.
  it('with the rows deleted there is no run, and the page says the graph holds none', async () => {
    const runs = await loadRunsFromGraph(store({ ids: 'runTs\n', suites: SUITE_HEAD + '\n', records: RECORD_HEAD + '\n' }));
    expect(runs).toEqual([]);
    expect(renderNightlyPage(null)).toContain('No nightly run in the graph yet');
    expect(renderNightlyPage(null, { missingRun: '2026-09-24T03:00:00' })).toContain('No rows for nightly run 2026-09-24T03:00:00 in the graph');
  });
  it('a store that does not answer is null, so the route refuses', async () => {
    expect(await loadRunsFromGraph(store({}))).toBeNull();
    expect(await loadRunsFromGraph(store({ ids: 'runTs\n2026-09-24T03:00:00\n', suites: null, records: '' }))).toBeNull();
  });
  it('summaries with commas and quotes survive the CSV', () => {
    const csv = `${SUITE_HEAD}\nr,1,npm,platform/api,kade,fail,"Tests: 2 failed, 198 passed (""x"")",1\n`;
    expect(csvRecords(csv)[0].sum).toBe('Tests: 2 failed, 198 passed ("x")');
  });
});
