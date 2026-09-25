// #4156 — the nightly run, read from the graph rows the run wrote.
//
// Jeff's spec line (2026-09-12): "summary reporting provided from logs and/or
// graph". The runner writes every SUITE row as a TestSuiteRun the moment it
// lands and the run's tail (tally, errors, completion) on its PipelineRun, so
// the page, the readout and the morning nudge read those rows. Nothing here
// reads nightly-suites.log: the flat log stays the operational record in Loki,
// and a store that does not answer is an error, never a fallback to the file.
//
// The rows become the same NightlyRun objects the page always rendered, so the
// renderer and the readout did not change: one record, two sources became one.
import { csvCells, type NightlyRun, type NightlyRow, type NightlyTally } from './nightly-report';
import type { NightlyRunRecord } from './nightly-readout';

/** A SPARQL read that answers CSV text, or null when the store did not answer. */
export type CsvQuery = (sparql: string) => Promise<string | null>;

const P = 'PREFIX c: <https://jeffbridwell.com/chorus#>';
const TESTS = '<urn:chorus:domains:tests>';
const PIPELINES = '<urn:chorus:domains:pipelines>';

/** The newest `limit` run ids that wrote suite rows, newest first. */
export function runIdsQuery(limit: number): string {
  return `${P} SELECT DISTINCT ?runTs WHERE { GRAPH ${TESTS} { ?r a c:TestSuiteRun ; c:runTs ?runTs ; c:suiteOrder ?o } }`
    + ` ORDER BY DESC(STR(?runTs)) LIMIT ${Math.max(1, Math.floor(limit))}`;
}

/** Every suite row of the runs from `oldest` on. */
export function suiteRowsQuery(oldest: string): string {
  return `${P} SELECT ?runTs ?order ?kind ?fp ?owner ?res ?sum ?ts WHERE { GRAPH ${TESTS} {`
    + ' ?r a c:TestSuiteRun ; c:runTs ?runTs ; c:suiteOrder ?order ; c:suiteKind ?kind ; c:filePath ?fp ;'
    + ' c:suiteOwner ?owner ; c:result ?res ; c:ts ?ts . OPTIONAL { ?r c:suiteSummary ?sum }'
    + ` FILTER(STR(?runTs) >= "${oldest.replace(/"/g, '')}") } }`;
}

/** The run records (the tail the run wrote when it ended or was stopped). */
export function runRecordsQuery(oldest: string): string {
  const opt = ['runOutcome', 'runCompletedAt', 'testsRegistered', 'testsRun', 'testsPassed', 'testsFailed',
    'testsUnmeasured', 'testsNoResult', 'failedCaseCount', 'exceptionCount', 'httpErrorCount',
    'assertionFailureCount', 'otherErrorCount'];
  return `${P} SELECT ?runTs ${opt.map((f) => `?${f}`).join(' ')} WHERE { GRAPH ${PIPELINES} {`
    + ' ?r a c:PipelineRun ; c:runTs ?runTs .'
    + opt.map((f) => ` OPTIONAL { ?r c:${f} ?${f} }`).join('')
    + ` FILTER(STR(?runTs) >= "${oldest.replace(/"/g, '')}") } }`;
}

/** CSV text to one record per row, keyed by the header. */
export function csvRecords(csv: string): Record<string, string>[] {
  const lines = csv.split('\n').map((l) => l.replace(/\r$/, '')).filter((l) => l !== '');
  if (lines.length === 0) return [];
  const head = csvCells(lines[0]);
  return lines.slice(1).map((l) => {
    const cells = csvCells(l);
    return Object.fromEntries(head.map((h, i) => [h, cells[i] ?? '']));
  });
}

const num = (v: string | undefined): number | undefined => {
  if (v === undefined || v === '') return undefined;
  const n = Number(v);
  return Number.isFinite(n) ? n : undefined;
};

/** The tally line, from the run record's numbers. Absent when the run took no
 *  reading, so the readout says "not measured" rather than zero. */
function tallyOf(rec: Record<string, string>): NightlyTally | undefined {
  const registered = num(rec.testsRegistered);
  if (registered === undefined) return undefined;
  const t = {
    registered, ran: num(rec.testsRun), passed: num(rec.testsPassed), failed: num(rec.testsFailed),
    unmeasured: num(rec.testsUnmeasured), noResult: num(rec.testsNoResult),
  };
  const s = (v: number | undefined) => (v === undefined ? '?' : String(v));
  return {
    ...t,
    text: `registered ${s(t.registered)} · ran ${s(t.ran)} · passed ${s(t.passed)} · failed ${s(t.failed)}`
      + ` · unmeasured ${s(t.unmeasured)} · no result ${s(t.noResult)}`,
  };
}

function errorsOf(rec: Record<string, string>): string | undefined {
  const fc = num(rec.failedCaseCount);
  if (fc === undefined) return undefined;
  const s = (k: string) => String(num(rec[k]) ?? 0);
  return `failed cases ${fc} · exceptions ${s('exceptionCount')} · http ${s('httpErrorCount')}`
    + ` · assertions ${s('assertionFailureCount')} · other ${s('otherErrorCount')}`;
}

/** Rows + records → runs, oldest first (the order the readout's history wants).
 *  A run with suite rows and no record has not finished: it is live or wedged,
 *  and quietForMs (from its newest row) tells the page which. */
export function runsFromGraph(suiteCsv: string, recordCsv: string, nowMs: number): NightlyRunRecord[] {
  const records = new Map(csvRecords(recordCsv).map((r) => [r.runTs, r]));
  const byRun = new Map<string, { rows: (NightlyRow & { order: number })[]; lastMs: number }>();
  for (const r of csvRecords(suiteCsv)) {
    const run = byRun.get(r.runTs) ?? { rows: [], lastMs: 0 };
    run.rows.push({
      order: num(r.order) ?? 0, kind: r.kind, path: r.fp, owner: r.owner, status: r.res, summary: r.sum ?? '',
    });
    run.lastMs = Math.max(run.lastMs, tsMs(r.ts));
    byRun.set(r.runTs, run);
  }
  const ids = [...byRun.keys()].sort();
  return ids.map((id, i) => {
    const got = byRun.get(id)!;
    const rec = records.get(id);
    const rows: NightlyRow[] = got.rows.sort((a, b) => a.order - b.order)
      .map(({ kind, path, owner, status, summary }) => ({ kind, path, owner, status, summary }));
    const run: NightlyRun & { runId: string } = { runId: id, startedAt: id, completed: false, rows };
    if (rec?.runOutcome === 'stopped') {
      run.stoppedAt = rec.runCompletedAt || undefined;
      run.stoppedDetail = 'stopped';
    } else if (rec) {
      run.completed = true;
      run.completedAt = rec.runCompletedAt || undefined;
    }
    if (rec) {
      const t = tallyOf(rec);
      if (t) run.tally = t;
      const e = errorsOf(rec);
      if (e) run.errors = e;
    }
    // #4009 — liveness belongs to the newest run only
    if (i === ids.length - 1 && !run.completed && got.lastMs > 0) {
      run.quietForMs = Math.max(0, nowMs - got.lastMs);
      run.lastRowAt = new Date(got.lastMs).toISOString();
    }
    return run;
  });
}

/** `ts` is epoch ms on nightly rows; an ISO stamp reads too. */
function tsMs(v: string | undefined): number {
  if (!v) return 0;
  const n = Number(v);
  if (Number.isFinite(n)) return n;
  const d = Date.parse(v);
  return Number.isFinite(d) ? d : 0;
}

/** The last `limit` runs from the graph, oldest first. null = the store did
 *  not answer (the caller refuses, it never falls back to the log). An empty
 *  list = the store answered and holds no run rows. */
export async function loadRunsFromGraph(query: CsvQuery, limit = 14, nowMs = Date.now()): Promise<NightlyRunRecord[] | null> {
  const idsCsv = await query(runIdsQuery(limit));
  if (idsCsv === null) return null;
  const ids = csvRecords(idsCsv).map((r) => r.runTs).filter(Boolean);
  if (ids.length === 0) return [];
  const oldest = [...ids].sort()[0];
  const [suites, records] = await Promise.all([query(suiteRowsQuery(oldest)), query(runRecordsQuery(oldest))]);
  if (suites === null || records === null) return null;
  return runsFromGraph(suites, records, nowMs);
}

/** The CsvQuery the routes use: Fuseki's query endpoint, CSV out. */
export function fusekiCsv(endpoint: string, fetchFn: typeof fetch = fetch): CsvQuery {
  return async (sparql: string) => {
    try {
      const r = await fetchFn(`${endpoint}?query=${encodeURIComponent(sparql)}`, {
        headers: { Accept: 'text/csv' }, signal: AbortSignal.timeout(20000) });
      return r.ok ? await r.text() : null;
    } catch { return null; }
  };
}
