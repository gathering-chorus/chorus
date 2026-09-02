// #4060 — the nightly READOUT. Jeff, 2026-09-02 06:11: "card i dont even get a
// readout on the run."
//
// Every night a run happened and what reached him was a count in a nudge; to
// learn more he asked a role, and each role re-derived its own numbers (13, 16
// and 5 for overlapping runs on 2026-09-01). This module is the ONE place a
// run's numbers are computed. The nudge Jeff receives, the /nightly page, the
// JSON a role reads when asked, and `nightly-suites.sh --readout` all call
// buildReadout on the same run record — so two roles asked about the same run
// give the same answer by construction, not by discipline.
//
// Units are Jeff's: minutes, suites, owners, and "what changed since last
// run". The record is the log nightly-suites.sh writes (RUN|start … RUN|
// complete blocks with SUITE| rows); every past run is still in it (#3709
// appends, never truncates), so history is the same file read fully.
import { parseNightlyLog, displayPath, type NightlyRun, type NightlyRow } from './nightly-report';

export type NightlyRunRecord = NightlyRun & { runId: string };

/** #4073 — the three things a red can mean, DERIVED from the run history
 *  (never declared by the test): the product broke, the test is wrong, or
 *  nothing was measured. */
export type RedLabel = 'product-broke' | 'test-wrong' | 'unmeasured';

export type RedSuite = { owner: string; suite: string; kind: string; label: RedLabel };

export type Readout = {
  runId: string;
  startedAt: string;
  completedAt: string | null;
  completed: boolean;
  /** null when the run never completed — a partial run has no honest duration */
  durationMin: number | null;
  suites: number;
  passed: number;
  failed: number;
  skipped: number;
  /** rows whose status is none of pass/fail/skip: produced no parseable output */
  silent: number;
  reds: RedSuite[];
  redByOwner: Record<string, number>;
  /** #4073 — how many reds of each label; the zero-red bar measures product-broke */
  byLabel: Record<RedLabel, number>;
  changes: {
    /** null = no earlier run in the record; the delta is then UNKNOWN, not zero */
    previousRunId: string | null;
    newlyRed: RedSuite[];
    fixed: RedSuite[];
    stillRed: RedSuite[];
    /** suites the previous run had that this run did not */
    gone: string[];
  };
};

/** Every run block in the log, oldest first. Each block is parsed by the same
 *  parser /nightly has always used (one verdict vocabulary, #3920); the id is
 *  the start timestamp, which is what the log itself uses to bracket a run. */
export function parseAllRuns(text: string): NightlyRunRecord[] {
  const runs: NightlyRunRecord[] = [];
  let block: string[] | null = null;
  const flush = () => {
    if (!block) return;
    const run = parseNightlyLog(block.join('\n'));
    if (run) runs.push({ ...run, runId: run.startedAt });
  };
  for (const l of text.split('\n')) {
    if (l.startsWith('RUN|start|')) { flush(); block = [l]; continue; }
    if (block) block.push(l);
  }
  flush();
  return runs;
}

/** "latest" or a run id. An unknown id is null — never silently the newest. */
export function findRun(runs: NightlyRunRecord[], id: string): NightlyRunRecord | null {
  if (!runs.length) return null;
  if (id === 'latest' || id === '') return runs[runs.length - 1];
  return runs.find((r) => r.runId === id) ?? null;
}

/** Failure text that names the machine's condition, not the product. */
const MACHINE_WORDS = /latency|timeout|timed out|under load|ECONNREFUSED|no live stack|stack-down|stack down|load \d|wedged/i;
/** Summaries where the runner took no reading at all. */
const NO_READING = /never ran|runner produced no|UNMEASURED|no parseable output|killed after|produced no results|0 pass, 0 fail/i;

/** #4073 — label one red from its own history (oldest → newest, the newest
 *  being this run's fail) and its summary line. Pure; the readout is the only
 *  caller. Rules, in order:
 *    unmeasured    the summary says the runner never took a reading
 *    test-wrong    the summary names the machine, or the suite flipped
 *                  pass/fail ≥2 times in its last 10 runs (whack-a-mole)
 *    product-broke everything else: a red that has held, or a first red
 *                  after a steady green history
 *  NEGATIVE PROOFS in nightly-red-labels-4073.test.ts: red-every-run is never
 *  test-wrong; a single red with no history is never unmeasured. */
export function labelRed(history: string[], summary: string): RedLabel {
  if (NO_READING.test(summary)) return 'unmeasured';
  if (MACHINE_WORDS.test(summary)) return 'test-wrong';
  const recent = history.slice(-10);
  let flips = 0;
  let prev = '';
  for (const v of recent) { if (prev && v !== prev) flips++; prev = v; }
  return flips >= 2 ? 'test-wrong' : 'product-broke';
}

const LABEL_WORDS: Record<RedLabel, string> = { 'product-broke': 'PRODUCT BROKE', 'test-wrong': 'TEST WRONG', unmeasured: 'UNMEASURED' };

/** The pass/fail history of one suite across the recorded runs up to and
 *  including `upTo` (oldest first), keyed by display path. */
function suiteHistory(history: NightlyRunRecord[], upTo: NightlyRunRecord, suite: string): string[] {
  const out: string[] = [];
  for (const r of history) {
    const row = r.rows.find((x) => displayPath(x.path) === suite);
    if (row && (row.status === 'pass' || row.status === 'fail')) out.push(row.status);
    if (r === upTo) break;
  }
  return out;
}

const toRed = (r: NightlyRow, label: RedLabel): RedSuite => ({ owner: r.owner, suite: displayPath(r.path), kind: r.kind, label });

function minutesBetween(a: string, b: string): number | null {
  const t0 = Date.parse(a), t1 = Date.parse(b);
  if (!Number.isFinite(t0) || !Number.isFinite(t1) || t1 < t0) return null;
  return Math.round((t1 - t0) / 60000);
}

/** The delta against the previous run. Keyed by display path so an absolute
 *  path from an older walker row matches its repo-relative successor. */
function diffRuns(rows: NightlyRow[], reds: RedSuite[], prev: NightlyRunRecord | null): Readout['changes'] {
  const changes: Readout['changes'] = { previousRunId: null, newlyRed: [], fixed: [], stillRed: [], gone: [] };
  if (!prev) return changes;
  changes.previousRunId = prev.runId;
  const prevRed = new Map(prev.rows.filter((r) => r.status === 'fail').map((r) => [displayPath(r.path), toRed(r, 'product-broke')]));
  const nowAll = new Set(rows.map((r) => displayPath(r.path)));
  const nowRed = new Set(reds.map((r) => r.suite));
  for (const r of reds) (prevRed.has(r.suite) ? changes.stillRed : changes.newlyRed).push(r);
  for (const [suite, red] of prevRed) {
    if (nowAll.has(suite) && !nowRed.has(suite)) changes.fixed.push(red);
  }
  changes.gone = prev.rows.map((r) => displayPath(r.path)).filter((s) => !nowAll.has(s));
  return changes;
}

const countStatus = (rows: NightlyRow[], status: string): number => rows.filter((r) => r.status === status).length;

export function buildReadout(run: NightlyRunRecord, prev: NightlyRunRecord | null, history: NightlyRunRecord[] = []): Readout {
  const rows = run.rows;
  const hist = history.length ? history : [run];
  const reds = rows.filter((r) => r.status === 'fail').map((r) => {
    const suite = displayPath(r.path);
    return toRed(r, labelRed(suiteHistory(hist, run, suite), r.summary));
  });
  const byLabel: Record<RedLabel, number> = { 'product-broke': 0, 'test-wrong': 0, unmeasured: 0 };
  for (const r of reds) {
    if (r.label === 'product-broke') byLabel['product-broke'] += 1;
    else if (r.label === 'test-wrong') byLabel['test-wrong'] += 1;
    else byLabel.unmeasured += 1;
  }
  const redByOwner: Record<string, number> = {};
  for (const r of reds) redByOwner[r.owner] = (redByOwner[r.owner] ?? 0) + 1;
  const completedAt = run.completed ? (run.completedAt ?? null) : null;
  return {
    runId: run.runId,
    startedAt: run.startedAt,
    completedAt,
    completed: run.completed,
    durationMin: completedAt ? minutesBetween(run.startedAt, completedAt) : null,
    suites: rows.length,
    passed: countStatus(rows, 'pass'),
    failed: reds.length,
    skipped: countStatus(rows, 'skip'),
    silent: rows.filter((r) => !['pass', 'fail', 'skip'].includes(r.status)).length,
    reds,
    redByOwner,
    byLabel,
    changes: diffRuns(rows, reds, prev),
  };
}

/** The one-paragraph readout: what Jeff receives as a nudge and what a role
 *  prints when asked. Same numbers as the JSON; this only formats them. */
function headLine(r: Readout): string {
  const when = r.startedAt.replace('T', ' ');
  if (!r.completed) return `nightly ${when} PARTIAL — never completed: ${r.suites} suites so far, ${r.failed} red so far`;
  const extras = [
    r.failed === 0 ? ' (all green)' : '',
    r.skipped ? `, ${r.skipped} skipped` : '',
    r.silent ? `, ${r.silent} no output` : '',
  ].join('');
  return `nightly ${when} took ${r.durationMin ?? '?'} min: ${r.suites} suites, ${r.failed} red${extras}`;
}

/** #4073 — the split line: "4 red: 2 product broke, 1 test wrong, 1 unmeasured". */
function labelLine(r: Readout): string {
  const b = r.byLabel;
  return `${r.failed} red: ${b['product-broke']} product broke, ${b['test-wrong']} test wrong, ${b.unmeasured} unmeasured`;
}

const redLine = (prefix: string, red: RedSuite): string => `  ${prefix}${red.owner.padEnd(6)} ${red.suite}`;

function deltaLines(c: Readout['changes']): string[] {
  if (c.previousRunId === null) return ['since last run: no earlier run to compare'];
  const gone = c.gone.length ? `, ${c.gone.length} suite(s) no longer run` : '';
  return [
    `since last run (${c.previousRunId.replace('T', ' ')}): ${c.newlyRed.length} new red, ${c.fixed.length} fixed, ${c.stillRed.length} still red${gone}`,
    ...c.newlyRed.map((red) => redLine('new    ', red)),
    ...c.fixed.map((red) => redLine('fixed  ', red)),
  ];
}

export function renderReadoutText(r: Readout, baseUrl: string): string {
  const lines: string[] = [headLine(r)];
  if (r.failed > 0) {
    const owners = Object.entries(r.redByOwner).sort((a, b) => b[1] - a[1]).map(([o, n]) => `${o} ${n}`).join(', ');
    lines.push(labelLine(r), `red by owner: ${owners}`,
      ...r.reds.map((red) => redLine(`${LABEL_WORDS[red.label].padEnd(14)} `, red)));
  }
  lines.push(...deltaLines(r.changes), `${baseUrl}/nightly?run=${r.runId}`);
  return lines.join('\n');
}
