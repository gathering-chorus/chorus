// #4156 — test seam: the rows a nightly run writes to the graph, made from a
// log-format fixture, and a stand-in store that answers the page's queries
// with them as Fuseki CSV. Lets the route tests prove the page renders from
// the graph with no log file present at all.
import http from 'http';
import type { AddressInfo } from 'net';
import { parseAllRuns } from '../../src/handlers/nightly-readout';

const cell = (v: string | number | undefined): string => {
  const s = v === undefined ? '' : String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};
const line = (xs: (string | number | undefined)[]): string => xs.map(cell).join(',');

export const SUITE_HEAD = 'runTs,order,kind,fp,owner,res,sum,ts';
export const RECORD_HEAD = 'runTs,runOutcome,runCompletedAt,testsRegistered,testsRun,testsPassed,testsFailed,'
  + 'testsUnmeasured,testsNoResult,failedCaseCount,exceptionCount,httpErrorCount,assertionFailureCount,otherErrorCount';

/** The graph rows a run would have written for each run in the log fixture. */
export function graphFromLog(log: string, rowTsMs = 1790000000000): { suites: string; records: string } {
  const suites = [SUITE_HEAD];
  const records = [RECORD_HEAD];
  for (const run of parseAllRuns(log)) {
    run.rows.forEach((r, i) => suites.push(line([run.runId, i + 1, r.kind, r.path, r.owner, r.status, r.summary, rowTsMs + i])));
    const e = run.errors?.match(/\d+/g)?.map(Number);
    const t = run.tally;
    if (run.completed || run.stoppedAt) {
      records.push(line([run.runId, run.stoppedAt ? 'stopped' : 'red', run.completedAt ?? run.stoppedAt,
        t?.registered, t?.ran, t?.passed, t?.failed, t?.unmeasured, t?.noResult,
        e?.[0], e?.[1], e?.[2], e?.[3], e?.[4]]));
    }
  }
  return { suites: suites.join('\n') + '\n', records: records.join('\n') + '\n' };
}

/** A stand-in Fuseki: answers the three nightly queries from the given CSV,
 *  and anything else (the failing-cases read) with an empty result. */
export async function fakeStore(g: { suites: string; records: string }): Promise<{ url: string; close(): Promise<void> }> {
  const server = http.createServer((req, res) => {
    const q = decodeURIComponent((req.url ?? '').split('query=')[1] ?? '');
    res.setHeader('Content-Type', 'text/csv');
    if (q.includes('SELECT DISTINCT ?runTs')) {
      const ids = [...new Set(g.suites.trim().split('\n').slice(1).map((l) => l.split(',')[0]))].sort().reverse();
      res.end(['runTs', ...ids].join('\n') + '\n');
    } else if (q.includes('c:TestSuiteRun')) res.end(g.suites);
    else if (q.includes('c:PipelineRun')) res.end(g.records);
    else res.end('fp,tn,res,why\n');
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${port}/pods/query`, close: () => new Promise((r) => server.close(() => r())) };
}
