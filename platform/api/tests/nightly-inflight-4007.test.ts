// @test-type: unit
// #4007 — Jeff opens one page and sees which suite is running and how long it
// has been running, and a hung suite reads WEDGED without anyone running ps.
// Hermetic: the store is a stub CsvQuery answering the queries the page sends.
import { loadRunsFromGraph, inFlightFromCsv, liveRowsQuery, type CsvQuery } from '../src/handlers/nightly-graph';
import { inFlightLine, isWedged, renderNightlyPage } from '../src/handlers/nightly-report';

const NOW = Date.parse('2026-10-10T03:30:00Z');
const RUN = '2026-10-10T03:00:03';
const SUITES = [
  'runTs,order,kind,fp,owner,res,sum,ts,secs',
  `${RUN},1,bats,platform/tests/a.bats,kade,pass,1 pass,${NOW - 60_000},2.0`,
].join('\n');
const RECORDS_OPEN = 'runTs,runOutcome,runCompletedAt';
const RECORDS_DONE = `runTs,runOutcome,runCompletedAt\n${RUN},complete,2026-10-10T05:00:00`;

const live = (rows: [string, number, string][]): string =>
  ['fp,ts,sum', ...rows.map(([fp, ago, sum]) => `${fp},${NOW - ago},${sum}`)].join('\n');

function store(records: string, liveCsv: string): CsvQuery & { sent: string[] } {
  const sent: string[] = [];
  const q = (async (sparql: string) => {
    sent.push(sparql);
    if (sparql.includes('SELECT DISTINCT ?runTs')) return `runTs\n${RUN}`;
    if (sparql.includes('c:PipelineRun')) return records;
    if (sparql.includes('"running"')) return liveCsv;
    return SUITES;
  }) as CsvQuery & { sent: string[] };
  q.sent = sent;
  return q;
}

describe('#4007 the page names the suites running now', () => {
  it('a live run shows each running suite with its time, longest first', async () => {
    const runs = await loadRunsFromGraph(store(RECORDS_OPEN, live([
      ['platform/tests/b.bats', 125_000, 'timeout=1200s'],
      ['werk-test#units', 30_000, 'timeout=1200s'],
    ])), 14, NOW);
    const run = runs![runs!.length - 1];
    expect(run.inFlight).toHaveLength(2);
    const line = inFlightLine(run);
    expect(line).toContain('Running now: platform/tests/b.bats 2m 5s · werk-test#units 30s');
    expect(line).not.toContain('WEDGED');
    expect(renderNightlyPage(run)).toContain('Running now:');
  });

  it('NEGATIVE PROOF: a suite hung past its own timeout reads WEDGED; the healthy one beside it does not', async () => {
    const runs = await loadRunsFromGraph(store(RECORDS_OPEN, live([
      ['platform/tests/hung.bats', 21 * 60_000, 'timeout=1200s'],
      ['platform/tests/ok.bats', 5 * 60_000, 'timeout=1200s'],
    ])), 14, NOW);
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

  it('a finished run asks for no live rows and shows no running line', async () => {
    const s = store(RECORDS_DONE, live([['platform/tests/stale.bats', 60_000, 'timeout=1200s']]));
    const runs = await loadRunsFromGraph(s, 14, NOW);
    const run = runs![runs!.length - 1];
    expect(run.completed).toBe(true);
    expect(s.sent.some((q) => q.includes('"running"'))).toBe(false);
    expect(inFlightLine(run)).toBe('');
  });

  it('the live query reads only rows with no suiteOrder, for one run', () => {
    const q = liveRowsQuery(RUN);
    expect(q).toContain('FILTER NOT EXISTS { ?r c:suiteOrder ?o }');
    expect(q).toContain(`= "${RUN}"`);
    expect(inFlightFromCsv(live([['a', 1000, '']]), NOW)).toEqual([{ unit: 'a', elapsedMs: 1000 }]);
  });
});
