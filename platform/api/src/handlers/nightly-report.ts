// #3920 — /nightly: the rendered viewing surface for the nightly run.
// One page answering "was the night green?" — verdict first, reds on top,
// typed skips counted, killed runs named PARTIAL. Reads the same log
// nightly-suites.sh writes (RUN|/SUITE| lines): the page renders the record,
// it never re-derives verdicts (one verdict vocabulary, #3920 fold).

export type NightlyRow = {
  kind: string;
  path: string;
  owner: string;
  status: string; // pass | fail | skip
  summary: string;
};

/** #4271 — the run's own count of TEST CASES, read verbatim from its RUN|tally
 *  line. Suites and tests are two units; the readout stated only the first, so
 *  the graph's "17 failed of 421" (suites) and the log's "51 failed of 9,417"
 *  (cases) had no common surface. Numbers are absent when the run could not
 *  take the reading — never zero, because zero is a measurement. */
export type NightlyTally = {
  /** the line exactly as the run wrote it */
  text: string;
  registered?: number;
  ran?: number;
  passed?: number;
  failed?: number;
  unmeasured?: number;
  noResult?: number;
};

/** Read one `RUN|tally|…` body. The runner writes
 *  "registered N · ran N · passed N · failed N · unmeasured N · no result N",
 *  or a sentence when the registry was unreadable — which yields the text and
 *  no numbers, so a caller can tell "could not measure" from "measured zero".
 */
export function parseTally(body: string): NightlyTally {
  // Segments are "<label> <number>", separated by '·'. Read as data, not with
  // a regex built from a label: a non-literal RegExp is a lint the ratchet
  // refuses, and splitting is both simpler and cheaper to read.
  const seen = new Map<string, number>();
  for (const seg of body.split('·')) {
    const parts = seg.trim().split(/\s+/);
    const n = Number(parts.pop());
    const label = parts.join(' ');
    if (label && Number.isInteger(n)) seen.set(label, n);
  }
  return {
    text: body,
    registered: seen.get('registered'),
    ran: seen.get('ran'),
    passed: seen.get('passed'),
    failed: seen.get('failed'),
    unmeasured: seen.get('unmeasured'),
    noResult: seen.get('no result'),
  };
}

export type NightlyRun = {
  startedAt: string;
  completedAt?: string;
  completed: boolean;
  /** #4035 — the run was STOPPED (a person or agent-state ended it). Distinct
   *  from wedged (no output, nobody ended it) and from still-running. */
  stoppedAt?: string;
  stoppedDetail?: string;
  rows: NightlyRow[];
  /** #4271 — the run's test-grain tally, verbatim. Absent when the run wrote
   *  no tally line; the readout then reports the test grain as unmeasured. */
  tally?: NightlyTally;
  /** #4009 — liveness. A run that never completed is either working or wedged,
   *  and the page could not tell them apart: on 2026-08-25 a lane sat silent
   *  for 38 minutes while a human was told three different things about it.
   *  quietForMs is the gap since the last row; the page names it. */
  quietForMs?: number;
  lastRowAt?: string;
};

/** One `SUITE|kind|path|owner|status|summary` row, or null for any other line.
 *  The summary may itself contain '|', so it takes everything after field 5. */
function parseSuiteLine(l: string): NightlyRow | null {
  if (!l.startsWith('SUITE|')) return null;
  const parts = l.split('|');
  if (parts.length < 6) return null;
  return {
    kind: parts[1],
    path: parts[2],
    owner: parts[3],
    status: parts[4],
    summary: parts.slice(5).join('|'),
  };
}

/** Parse the LAST run block (RUN|start … RUN|complete) from the nightly log. */
export function parseNightlyLog(text: string): NightlyRun | null {
  const all = text.split('\n');
  const start = all.reduce((acc, l, i) => (l.startsWith('RUN|start|') ? i : acc), -1);
  if (start === -1) return null;
  const startLine = all.slice(start, start + 1).join('');
  const run: NightlyRun = {
    startedAt: startLine.split('|')[2] ?? '?',
    completed: false,
    rows: [],
  };
  for (const l of all.slice(start + 1)) {
    if (l.startsWith('RUN|complete|')) {
      run.completed = true;
      run.completedAt = l.split('|')[2];
      break;
    }
    if (l.startsWith('RUN|stopped|')) {
      // #4035 — the wrapper's stop handler wrote this; the block ends here.
      run.stoppedAt = l.split('|')[2];
      run.stoppedDetail = l.split('|')[3] ?? '';
      break;
    }
    // #4271 — the tally is kept alongside SUITE rows. The parser used to drop
    // it, so the readout never had the test grain to state.
    if (l.startsWith('RUN|tally|')) {
      run.tally = parseTally(l.slice('RUN|tally|'.length));
      continue;
    }
    const row = parseSuiteLine(l);
    if (row) run.rows.push(row);
  }
  // #4009 — how long has this run been silent? Rows carry no timestamps, so the
  // honest source is the log file's own last write, supplied by the caller.
  return run;
}

/** #4009 — a run is WEDGED-LOOKING when it never completed and nothing has been
 *  written for longer than the threshold. Not a verdict on the code — a verdict
 *  on the RUN, which is exactly the distinction that was missing. */
export function quietVerdict(run: NightlyRun, quietMs: number, thresholdMs = 10 * 60 * 1000):
  'complete' | 'live' | 'quiet' {
  if (run.completed) return 'complete';
  return quietMs >= thresholdMs ? 'quiet' : 'live';
}

const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** #3964 — display form of a suite path: repo-relative, phone-readable.
 *  Old walker rows carry absolute paths; strip the chorus root, label the
 *  app root, and fall back to home-stripping so /Users never hits a screen. */
export function displayPath(p: string): string {
  const m = /^\/[^]*?\/CascadeProjects\/(chorus\/|jeff-bridwell-personal-site(\/|$)|[^/]+\/?)/.exec(p);
  if (!m) return p;
  if (m[1] === 'chorus/') return p.slice(m[0].length) || 'chorus';
  if (m[1].startsWith('jeff-bridwell-personal-site')) {
    const rest = p.slice(m[0].length);
    return rest ? `app:${rest}` : 'app:jeff-bridwell-personal-site';
  }
  return p.slice(m.index + m[0].length - m[1].length);
}

/** Two different things get counted on this page and they were never labelled:
 *  a SUITE is one file or project that runs; a TEST is one check inside it.
 *  The headline counted suites, the rows printed tests, and "13 failed" sat
 *  next to "179 fail" with no unit on either. This adds the test-level tally
 *  so both units are on the page, each named. */
export function tallyTests(rows: NightlyRow[]): {
  passed: number; failed: number; unparsed: number;
} {
  let passed = 0, failed = 0, unparsed = 0;
  for (const r of rows) {
    const s = r.summary;
    // jest/vitest: "Tests: 74 failed, 27 skipped, 4562 passed, 4663 total".
    // Read the two numbers independently rather than with chained optional
    // groups — that form is what security/detect-unsafe-regex flagged, and it
    // is also easier to read than one pattern spanning an optional middle.
    const jestPassed = s.startsWith('Tests:') ? /(\d+) passed/.exec(s) : null;
    const jestFailed = s.startsWith('Tests:') ? /(\d+) failed/.exec(s) : null;
    // bats: "bats: 10 passed, 4 failed"
    const bats = /bats: *(\d+) passed, *(\d+) failed/.exec(s);
    // shell/cargo/reconcile: "13 pass, 1 fail"
    const plain = /(\d+) +pass(?:ed)?, *(\d+) +fail/.exec(s);
    if (jestPassed) {
      passed += Number(jestPassed[1]);
      if (jestFailed) failed += Number(jestFailed[1]);
    } else if (bats) { passed += Number(bats[1]); failed += Number(bats[2]); }
    else if (plain) { passed += Number(plain[1]); failed += Number(plain[2]); }
    else unparsed += 1;
  }
  return { passed, failed, unparsed };
}

/** #4060 — what the page shows above the run: the readout (duration, delta vs
 *  the previous run) and the list of every recorded run, so any past night is
 *  one click away. Typed structurally to keep this module free of a cycle
 *  with nightly-readout.ts, which imports the parser from here. */
export type NightlyPageOpts = {
  readout?: {
    runId: string;
    durationMin: number | null;
    failed?: number;
    /** #4073 — counts by derived label; rendered as the split line */
    byLabel?: { 'product-broke': number; 'test-wrong': number; unmeasured: number };
    reds?: { suite: string; label: string }[];
    changes: {
      previousRunId: string | null;
      newlyRed: { owner: string; suite: string }[];
      fixed: { owner: string; suite: string }[];
      stillRed: { owner: string; suite: string }[];
      gone: string[];
    };
  };
  history?: { runId: string; completed: boolean; rows: NightlyRow[] }[];
  /** #4277 — failing cases per suite path, from the run's TestResult rows */
  cases?: CasesBySuite;
  /** #4277 — the type a suite ran as (declared header for file suites) */
  typeOf?: (r: NightlyRow) => string;
};

/** #4073 — "4 red: 2 product broke, 1 test wrong, 1 unmeasured", derived from
 *  run history. Empty when the readout carries no split (older callers). */
function splitLine(r: NonNullable<NightlyPageOpts['readout']>): string {
  const b = r.byLabel;
  if (!b || !r.failed) return '';
  return `<span class="split"><b>${r.failed} red:</b> ${b['product-broke']} product broke · ${b['test-wrong']} test wrong · ${b.unmeasured} unmeasured</span>`;
}

function labelText(label: string): string {
  if (label === 'product-broke') return 'PRODUCT BROKE';
  if (label === 'test-wrong') return 'TEST WRONG';
  return label === 'unmeasured' ? 'UNMEASURED' : '';
}

function renderHistory(o: NightlyPageOpts | undefined, current: string): string {
  const h = o?.history;
  if (!h || h.length < 1) return '';
  const items = [...h].reverse().map((run) => {
    const reds = run.rows.filter((r) => r.status === 'fail').length;
    const label = run.completed ? `${reds} red / ${run.rows.length}` : `partial (${run.rows.length} so far)`;
    const cls = run.runId === current ? ' class="cur"' : '';
    return `<li${cls}><a href="/nightly?run=${esc(run.runId)}">${esc(run.runId)}</a> <span class="hl">${esc(label)}</span></li>`;
  }).join('');
  // #4277 — history is the LAST fold on the page, after every suite.
  return `<details class="history"><summary><span class="lbl">${h.length} recorded runs</span><span class="hint">open any</span></summary><ul>${items}</ul></details>`;
}

/** #4063/#4073 — the run's verdict and banner class, pulled out so
 *  renderNightlyPage stays under the complexity cap (the ratchet on main went
 *  +1 on it, 2026-09-02). A partial run has NO verdict (IN PROGRESS); green is
 *  only ever said of a whole night. */
function runVerdict(run: NightlyRun, reds: number): { verdict: string; cls: string } {
  if (!run.completed) {
    return { verdict: `IN PROGRESS — ${run.rows.length} suite(s) so far, ${reds} red so far`, cls: 'partial' };
  }
  return reds === 0 ? { verdict: 'ALL GREEN', cls: 'green' } : { verdict: `${reds} RED SUITES`, cls: 'red' };
}

/** The not-finished line: STOPPED (#4035), NO OUTPUT (#4009 wedged), or
 *  RUNNING. Empty for a completed run. #4277 — a line INSIDE the one banner,
 *  not a second banner above it (two grey bars said the same thing twice). */
function notFinishedLine(run: NightlyRun): string {
  if (run.completed) return '';
  if (run.stoppedAt) {
    return `<span class="state">STOPPED at ${esc(run.stoppedAt)}${run.stoppedDetail ? ' (' + esc(run.stoppedDetail) + ')' : ''} — not a full night.</span>`;
  }
  const quiet = run.quietForMs ?? 0;
  const mins = Math.round(quiet / 60000);
  return quietVerdict(run, quiet) === 'quiet'
    ? `<span class="state">NO OUTPUT for ${mins} min — started ${esc(run.startedAt)} and nothing since. Treat it as wedged, not slow.</span>`
    : `<span class="state">RUNNING — started ${esc(run.startedAt)}, last result ${mins} min ago.</span>`;
}

// ---------------------------------------------------------------------------
// #4277 — suites folded by TEST TYPE, in the order the run executed them.

/** Kinds that are a unit of code (the runner names the tool) → their layer.
 *  Lane kinds (coverage, security, perf, ui, bdd, …) ARE the type. */
const KIND_LAYER = new Map<string, string>([
  ['cargo', 'unit'], ['npm', 'unit'], ['app-eslint', 'lint'], ['coverage-denominator', 'coverage'],
]);
const FILE_KINDS = new Set(['shell', 'bats']);
const LAYERS = new Set(['unit', 'integration', 'bdd', 'e2e', 'contract', 'fitness', 'smoke']);

export type ReadFile = (path: string) => string | null;

/** #4277 — the type a suite ran AS. File-backed suites (shell, bats) declare
 *  it in their leading comment (`@test-type: <layer>[:concern]`, the #3442
 *  gate's grammar); the tool name is NOT a layer. A file with no declaration
 *  is named `undeclared (<tool>)` so it can never pose as one. */
export function suiteType(row: { kind: string; path: string }, readFile: ReadFile): string {
  if (!FILE_KINDS.has(row.kind)) return KIND_LAYER.get(row.kind) ?? row.kind;
  const text = readFile(row.path);
  const declared = text === null ? null : parseDeclaration(text);
  return declared && LAYERS.has(declared) ? declared : `undeclared (${row.kind})`;
}

/** The layer from a file's leading comment block, or null. Mirrors
 *  gate-test-type's honored-only-in-the-header rule: once real code starts,
 *  the search stops. */
export function parseDeclaration(content: string): string | null {
  for (const raw of content.split('\n').slice(0, 40)) {
    const line = raw.trim();
    if (line === '' || line.startsWith('#!')) continue;
    if (!/^(\/\/|#|\/\*|\*)/.test(line)) return null;
    const m = /^(?:\/\/|#|\*|\/\*)\s*@test-type:\s*([a-z0-9-]+)/i.exec(line);
    if (m) return m[1].toLowerCase();
  }
  return null;
}

export function groupByType<T extends { kind: string; path: string }>(
  rows: T[], typeOf: (r: T) => string,
): { type: string; rows: T[] }[] {
  const groups: { type: string; rows: T[] }[] = [];
  for (const r of rows) {
    const t = typeOf(r);
    let g = groups.find((x) => x.type === t);
    if (!g) { g = { type: t, rows: [] }; groups.push(g); }
    g.rows.push(r);
  }
  return groups;
}

/** #4277 — "72.53210748305766%" → "72.5%". A percentage is a reading, not a
 *  hash; one decimal is what a person compares against a floor. */
export function oneDecimal(summary: string): string {
  return summary.replace(/(\d+)\.(\d+)%/g, (_m, i: string, f: string) => `${Number(`${i}.${f}`).toFixed(1)}%`);
}

// ---------------------------------------------------------------------------
// #4277 — failing cases, read from the TestResult rows the runner writes.

export type CaseRow = { name: string; result: string };
export type CasesBySuite = Record<string, CaseRow[]>;

/** One query per page, bounded to the run's own window (the #4015 lesson: an
 *  unbounded ?ts counted 190k historical rows). Passes are excluded server-side
 *  so the page parses only what it shows. */
export function failingCasesQuery(run: { startedAt: string; completedAt?: string }): string {
  const ended = run.completedAt ?? '9999';
  return 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?fp ?tn ?res WHERE { GRAPH <urn:chorus:domains:tests> {'
    + ' ?r a c:TestResult ; c:runTs ?ts ; c:filePath ?fp ; c:testName ?tn ; c:result ?res'
    + ` FILTER(STR(?ts) >= "${run.startedAt}" && STR(?ts) <= "${ended}") FILTER(?res != "pass") } } ORDER BY ?fp`;
}

/** Fuseki CSV → cases grouped by suite path. Handles quoted fields (a case
 *  name may carry commas) and the \r Fuseki ends lines with. */
export function parseFailingCases(csv: string): CasesBySuite {
  const out = new Map<string, CaseRow[]>();
  const lines = csv.split('\n').map((l) => l.replace(/\r$/, '')).filter((l) => l !== '');
  for (const line of lines.slice(1)) {
    const cells = csvCells(line);
    if (cells.length < 3) continue;
    const [fp, tn, res] = cells;
    const list = out.get(fp) ?? [];
    list.push({ name: tn, result: res });
    out.set(fp, list);
  }
  return Object.fromEntries(out);
}

function csvCells(line: string): string[] {
  // one cell per match: a quoted field (doubled quotes inside) or a bare run to the next comma
  const cells: string[] = [];
  const re = /"((?:[^"]|"")*)"|([^,]*)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(line)) !== null) {
    cells.push(m[0].startsWith('"') ? m[1].replace(/""/g, '"') : m[2]);
    if (line.charAt(re.lastIndex) === ',') re.lastIndex += 1;
    else break;
  }
  return cells;
}

export type FetchLike = (url: string, init?: { headers?: Record<string, string>; signal?: AbortSignal }) => Promise<{ ok: boolean; text(): Promise<string> }>;

/** The store read the route makes. A store that does not answer yields no
 *  cases (the page then says "no case rows recorded"), never a fabricated list. */
export async function fetchFailingCases(
  run: { startedAt: string; completedAt?: string }, fuseki: string, fetchFn: FetchLike,
): Promise<CasesBySuite> {
  try {
    const r = await fetchFn(`${fuseki}?query=${encodeURIComponent(failingCasesQuery(run))}`, {
      headers: { Accept: 'text/csv' }, signal: AbortSignal.timeout(20000) });
    if (!r.ok) return {};
    return parseFailingCases(await r.text());
  } catch { return {}; }
}

// ---------------------------------------------------------------------------
// the page

const PILL = new Map<string, string>([['fail', 'red'], ['pass', 'green'], ['skip', 'amber'], ['slow', 'amber']]);
const pillClass = (status: string): string => PILL.get(status) ?? 'unm';

function suiteLine(r: NightlyRow): string {
  return `<li class="suite ${esc(r.status)}"><span class="pill ${pillClass(r.status)}">${esc(r.status)}</span>`
    + `<span class="kind">${esc(r.kind)}</span><span class="path">${esc(displayPath(r.path))}</span>`
    + `<span class="owner">${esc(r.owner)}</span><span class="sum">${esc(oneDecimal(r.summary))}</span></li>`;
}

/** lookup without a computed member access (the object-injection lint) */
function casesFor(cases: CasesBySuite | undefined, p: string): CaseRow[] | undefined {
  return cases ? Object.entries(cases).find(([k]) => k === p)?.[1] : undefined;
}

/** A shell suite records ONE result row named for the file — that is the
 *  suite's own verdict, not a case inside it. Showing it as a case would
 *  print the suite's name twice and tell the reader nothing new. */
function isSuiteOwnRow(cases: CaseRow[], r: NightlyRow): boolean {
  const file = r.path.split('/').pop() ?? r.path;
  return cases.length === 1 && cases[0].name === file;
}

function caseList(cases: CaseRow[] | undefined, r: NightlyRow): string {
  if (!cases || cases.length === 0 || isSuiteOwnRow(cases, r)) {
    return `<li class="case none">no case rows recorded for this suite — the summary is all the run wrote: ${esc(oneDecimal(r.summary))}</li>`;
  }
  return cases.map((c) => `<li class="case ${esc(c.result)}"><span class="m">${esc(c.result)}</span><span>${esc(c.name)}</span></li>`).join('');
}

function redFold(r: NightlyRow, o: NightlyPageOpts | undefined): string {
  const hit = o?.readout?.reds?.find((x) => x.suite === displayPath(r.path));
  const label = hit?.label ?? '';
  const cases = casesFor(o?.cases, r.path) ?? casesFor(o?.cases, displayPath(r.path));
  return `<details class="red" open><summary><span class="pill red">fail</span><span class="kind">${esc(r.kind)}</span>`
    + `<span class="path">${esc(displayPath(r.path))}</span><span class="label ${esc(label)}">${labelText(label)}</span>`
    + `<span class="sum">${esc(r.owner)} · ${esc(oneDecimal(r.summary))}</span></summary><ul class="cases">${caseList(cases, r)}</ul></details>`;
}

function statusFold(rows: NightlyRow[], status: string, hint: string): string {
  if (rows.length === 0) return '';
  return `<details class="sub"><summary><span class="pill ${pillClass(status)}">${esc(status)}</span><span class="n">${rows.length}</span>`
    + `<span class="lbl">${esc(status)}</span><span class="hint">${hint}</span></summary><ul class="suites">${rows.map(suiteLine).join('')}</ul></details>`;
}

function typeFold(g: { type: string; rows: NightlyRow[] }, o: NightlyPageOpts | undefined): string {
  const by = (st: string) => g.rows.filter((r) => r.status === st);
  const reds = by('fail');
  const other = g.rows.filter((r) => !['fail', 'slow', 'skip', 'pass'].includes(r.status));
  const state = reds.length ? `<span class="pill red">${reds.length} red</span>` : '<span class="pill green">green</span>';
  const inner = reds.map((r) => redFold(r, o)).join('')
    + statusFold(by('slow'), 'slow', 'speed, not breakage')
    + statusFold(other, 'unmeasured', 'the check could not take a reading — not a pass')
    + statusFold(by('skip'), 'skip', 'typed skips')
    + statusFold(by('pass'), 'pass', 'one line each');
  return `<details class="group"${reds.length ? ' open' : ''}><summary>${state}<span class="n">${g.rows.length}</span>`
    + `<span class="lbl">${esc(g.type)}</span></summary>${inner}</details>`;
}

function renderBanner(run: NightlyRun, o: NightlyPageOpts | undefined): string {
  const reds = run.rows.filter((r) => r.status === 'fail').length;
  const counts = (st: string) => run.rows.filter((r) => r.status === st).length;
  const other = run.rows.filter((r) => !['pass', 'fail', 'skip', 'slow'].includes(r.status)).length;
  const t = run.tally;
  const { verdict, cls } = runVerdict(run, reds);
  const r = o?.readout;
  const dur = r ? (r.durationMin === null ? ' · duration unknown (run never completed)' : ` · took ${r.durationMin} min`) : '';
  const tests = t
    ? `<span><span class="u">tests</span> ${n(t.passed)} passed · ${n(t.failed)} failed · ${n(t.unmeasured)} unmeasured · ${n(t.noResult)} no result · ${n(t.ran)} ran of ${n(t.registered)} registered</span>`
    : '<span><span class="u">tests</span> unmeasured — the run wrote no tally line</span>';
  return `<div class="banner ${cls}">
  <div class="verdict">${verdict}</div>
  <div class="when">${esc(run.startedAt)}${run.completedAt ? ' → ' + esc(run.completedAt.slice(11)) : ''}${dur}</div>
  ${notFinishedLine(run)}
  <div class="counts"><span><span class="u">suites</span> ${counts('pass')} passed · ${reds} failed · ${counts('slow')} slow · ${other} unmeasured · ${counts('skip')} skipped</span>${tests}</div>
  ${r ? `<div class="split">${splitLine(r)}${deltaLine(r)}</div>` : ''}
</div>`;
}

const n = (v: number | undefined): string => (v === undefined ? '?' : v.toLocaleString('en-US'));

function deltaLine(r: NonNullable<NightlyPageOpts['readout']>): string {
  const c = r.changes;
  if (c.previousRunId === null) return '<span class="d">since last run: no earlier run to compare</span>';
  const detail = [
    ...c.newlyRed.map((x) => `<li class="new">new: ${esc(x.owner)} ${esc(x.suite)}</li>`),
    ...c.fixed.map((x) => `<li class="fixed">fixed: ${esc(x.owner)} ${esc(x.suite)}</li>`),
  ].join('');
  return `<span class="d">since <a href="/nightly?run=${esc(c.previousRunId)}">${esc(c.previousRunId)}</a>: `
    + `<span class="new">${c.newlyRed.length} new red</span> · <span class="fx">${c.fixed.length} fixed</span> · ${c.stillRed.length} still red`
    + (c.gone.length ? ` · ${c.gone.length} no longer run` : '') + '</span>'
    + (detail ? `<ul class="delta">${detail}</ul>` : '');
}

/** Render the run as the one-look report page. */
export function renderNightlyPage(run: NightlyRun | null, opts?: NightlyPageOpts): string {
  if (!run) {
    return page('Nightly', '<div class="banner empty">No nightly run recorded yet — first run lands at 03:00.</div>');
  }
  const reds = run.rows.filter((r) => r.status === 'fail');
  const { verdict } = runVerdict(run, reds.length);
  const typeOf = opts?.typeOf ?? ((r: NightlyRow) => KIND_LAYER.get(r.kind) ?? r.kind);
  const groups = groupByType(run.rows, typeOf).map((g) => typeFold(g, opts)).join('');
  const body = `
  ${renderBanner(run, opts)}
  ${groups}
  ${renderHistory(opts, run.startedAt)}
  <p class="prov">Suites in the order the run executed them; the page renders the record the run wrote (<code>werk-test --nightly</code>) and never re-derives a verdict. Failing cases are the run's own TestResult rows.</p>`;
  return page(`Nightly — ${verdict}`, body);
}

function page(title: string, body: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>
  :root { --bg:#f6f7f9; --panel:#fff; --fg:#1b1d22; --mut:#646b78; --line:#e1e4ea; --red:#c0392b; --red-bg:#fbeae7; --green:#1e7d32; --green-bg:#e8f3ea; --amber:#8a6d1a; --amber-bg:#f7f0dc; --unm:#5b4fa8; --unm-bg:#ebe8f7; --accent:#1f5f8b; }
  @media (prefers-color-scheme: dark) { :root { --bg:#141619; --panel:#1c1f24; --fg:#e9ebef; --mut:#9aa2b1; --line:#2b2f37; --red:#ff6b5e; --red-bg:#3a1f1c; --green:#5dd879; --green-bg:#1b3021; --amber:#e3c05a; --amber-bg:#3a3117; --unm:#a99cf2; --unm-bg:#26223d; --accent:#7fb3dc; } }
  body { background:var(--bg); color:var(--fg); font:15px/1.5 -apple-system,system-ui,sans-serif; max-width:64rem; margin:0 auto; padding:1.5rem 16px 3rem; display:flex; flex-direction:column; gap:1rem; }
  .banner { background:var(--panel); border:1px solid var(--line); border-left:6px solid var(--mut); border-radius:10px; padding:1rem 1.25rem; display:grid; grid-template-columns:minmax(0,1fr) auto; gap:.25rem 1.5rem; align-items:baseline; min-width:0; }
  .banner > * { min-width:0; overflow-wrap:anywhere; }
  .banner.red { border-left-color:var(--red); } .banner.green { border-left-color:var(--green); } .banner.partial, .banner.empty { border-left-color:var(--amber); }
  .verdict { font-size:1.7rem; font-weight:700; }
  .banner.red .verdict { color:var(--red); } .banner.green .verdict { color:var(--green); } .banner.partial .verdict { color:var(--amber); }
  .when { color:var(--mut); font-family:ui-monospace,monospace; font-size:.85rem; text-align:right; max-width:22rem; }
  .state { grid-column:1/-1; color:var(--amber); font-weight:600; }
  .counts { grid-column:1/-1; display:flex; flex-wrap:wrap; gap:.35rem 1.25rem; color:var(--mut); font-variant-numeric:tabular-nums; }
  .counts .u { color:var(--fg); font-weight:600; }
  .split { grid-column:1/-1; display:flex; flex-wrap:wrap; gap:.5rem 1rem; align-items:baseline; font-size:.9rem; }
  .split .d { color:var(--mut); } .split .new, .new { color:var(--red); } .split .fx, .fixed { color:var(--green); }
  .split b { color:var(--fg); }
  ul.delta { margin:0; padding-left:1.25rem; flex-basis:100%; }
  details.group, details.history { background:var(--panel); border:1px solid var(--line); border-radius:10px; }
  details.group > summary, details.history > summary, details.sub > summary, details.red > summary { list-style:none; cursor:pointer; }
  details > summary::-webkit-details-marker { display:none; }
  details.group > summary, details.history > summary, details.sub > summary { display:flex; align-items:center; gap:.75rem; padding:.7rem 1rem; font-weight:600; }
  details.group > summary::before, details.sub > summary::before { content:""; width:.5rem; height:.5rem; border-right:2px solid var(--mut); border-bottom:2px solid var(--mut); transform:rotate(-45deg); }
  details.group[open] > summary::before, details.sub[open] > summary::before { transform:rotate(45deg); }
  details.sub { border-top:1px solid var(--line); } details.sub > summary { padding-left:1.5rem; font-weight:500; }
  summary .n { font-variant-numeric:tabular-nums; min-width:3ch; text-align:right; } summary .lbl { flex:1; } summary .hint { color:var(--mut); font-weight:400; font-size:.85rem; min-width:0; }
  .pill { font-size:.7rem; font-weight:700; letter-spacing:.06em; text-transform:uppercase; padding:.1rem .45rem; border-radius:999px; white-space:nowrap; }
  .pill.red { background:var(--red-bg); color:var(--red); } .pill.green { background:var(--green-bg); color:var(--green); } .pill.amber { background:var(--amber-bg); color:var(--amber); } .pill.unm { background:var(--unm-bg); color:var(--unm); }
  ul.suites { border-top:1px solid var(--line); margin:0; padding:0; list-style:none; }
  li.suite, details.red > summary { display:grid; grid-template-columns:auto 5.5rem minmax(0,1fr) minmax(0,auto); gap:.25rem 1rem; padding:.45rem 1rem; border-bottom:1px solid var(--line); align-items:baseline; }
  li.suite > *, details.red > summary > * { min-width:0; }
  li.suite:last-child { border-bottom:0; }
  details.red { border-top:1px solid var(--line); } details.red > summary { background:var(--red-bg); } details.red > summary .path { font-weight:600; }
  .kind { color:var(--mut); font-size:.8rem; font-family:ui-monospace,monospace; }
  .path { font-family:ui-monospace,monospace; font-size:.85rem; overflow-wrap:anywhere; }
  .owner { color:var(--mut); font-size:.8rem; }
  .sum { grid-column:2/-1; color:var(--mut); font-size:.85rem; overflow-wrap:anywhere; }
  .label { font-size:.7rem; font-weight:700; letter-spacing:.05em; }
  .label.product-broke { color:var(--red); } .label.test-wrong { color:var(--amber); } .label.unmeasured { color:var(--unm); }
  ul.cases { margin:0; padding:.25rem 1rem .6rem 3rem; list-style:none; display:flex; flex-direction:column; gap:.2rem; }
  .case { display:flex; gap:.6rem; align-items:baseline; font-size:.9rem; overflow-wrap:anywhere; }
  .case .m { font-family:ui-monospace,monospace; font-size:.8rem; min-width:3.5ch; font-weight:700; }
  .case.fail .m { color:var(--red); } .case.skip .m { color:var(--amber); } .case.none { color:var(--mut); font-style:italic; }
  details.history { color:var(--mut); } details.history ul { columns:2; padding-left:1.25rem; margin:.25rem 1rem 1rem; } details.history li.cur { font-weight:700; color:var(--fg); }
  details.history a { color:var(--accent); text-decoration:none; font-family:ui-monospace,monospace; font-size:.85rem; } .hl { font-size:.85rem; }
  .prov { color:var(--mut); font-size:.8rem; margin:0; } code { font-family:ui-monospace,monospace; }
  @media (max-width:640px) { .banner { grid-template-columns:1fr; } .when { text-align:left; } li.suite, details.red > summary { grid-template-columns:auto 1fr; } .sum { grid-column:1/-1; } ul.cases { padding-left:1rem; } details.history ul { columns:1; } }
</style></head><body>${body}</body></html>`;
}
