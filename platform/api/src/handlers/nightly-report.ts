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

export type NightlyRun = {
  startedAt: string;
  completedAt?: string;
  completed: boolean;
  /** #4035 — the run was STOPPED (a person or agent-state ended it). Distinct
   *  from wedged (no output, nobody ended it) and from still-running. */
  stoppedAt?: string;
  stoppedDetail?: string;
  rows: NightlyRow[];
  /** #4009 — liveness. A run that never completed is either working or wedged,
   *  and the page could not tell them apart: on 2026-08-25 a lane sat silent
   *  for 38 minutes while a human was told three different things about it.
   *  quietForMs is the gap since the last row; the page names it. */
  quietForMs?: number;
  lastRowAt?: string;
};

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
    if (l.startsWith('SUITE|')) {
      // summary may itself contain '|'-free text; split into 6 parts max.
      const parts = l.split('|');
      if (parts.length >= 6) {
        run.rows.push({
          kind: parts[1],
          path: parts[2],
          owner: parts[3],
          status: parts[4],
          summary: parts.slice(5).join('|'),
        });
      }
    }
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
};

function renderReadoutBanner(o: NightlyPageOpts | undefined): string {
  const r = o?.readout;
  if (!r) return '';
  const c = r.changes;
  const dur = r.durationMin === null ? 'duration unknown (run never completed)' : `${r.durationMin} min`;
  const delta = c.previousRunId === null
    ? 'since last run: no earlier run to compare'
    : `since <a href="/nightly?run=${esc(c.previousRunId)}">${esc(c.previousRunId)}</a>: ` +
      `${c.newlyRed.length} new red · ${c.fixed.length} fixed · ${c.stillRed.length} still red` +
      (c.gone.length ? ` · ${c.gone.length} no longer run` : '');
  const detail = [
    ...c.newlyRed.map((x) => `<li class="new">new: ${esc(x.owner)} ${esc(x.suite)}</li>`),
    ...c.fixed.map((x) => `<li class="fixed">fixed: ${esc(x.owner)} ${esc(x.suite)}</li>`),
  ].join('');
  return `<div class="banner readout"><span>took ${esc(dur)}</span>${splitLine(r)}<span>${delta}</span>${detail ? `<ul class="delta">${detail}</ul>` : ''}</div>`;
}

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

/** the label cell for a red row; blank for non-red rows */
function labelCell(o: NightlyPageOpts | undefined, r: NightlyRow): string {
  if (r.status !== 'fail') return '<td class="lbl"></td>';
  const hit = o?.readout?.reds?.find((x) => x.suite === displayPath(r.path));
  const label = hit?.label ?? '';
  return `<td class="lbl ${esc(label)}">${labelText(label)}</td>`;
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
  return `<details class="history"><summary>${h.length} recorded run(s) — open any</summary><ul>${items}</ul></details>`;
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

/** The not-finished banner: STOPPED (#4035), NO OUTPUT (#4009 wedged), or
 *  RUNNING. Empty for a completed run. */
function notFinishedBanner(run: NightlyRun): string {
  if (run.completed) return '';
  if (run.stoppedAt) {
    return `<div class="banner partial">STOPPED at ${esc(run.stoppedAt)}${run.stoppedDetail ? ' (' + esc(run.stoppedDetail) + ')' : ''} — not a full night; the suites below ran before the stop.</div>`;
  }
  const quiet = run.quietForMs ?? 0;
  const mins = Math.round(quiet / 60000);
  return quietVerdict(run, quiet) === 'quiet'
    ? `<div class="banner partial">NO OUTPUT for ${mins} min — this run started ${esc(run.startedAt)} and has emitted nothing since. Treat it as wedged, not slow.</div>`
    : `<div class="banner partial">RUNNING — started ${esc(run.startedAt)}, last result ${mins} min ago. ${run.rows.length} suite(s) so far; not a full night yet.</div>`;
}

/** Render the run as the one-look report page. */
export function renderNightlyPage(run: NightlyRun | null, opts?: NightlyPageOpts): string {
  if (!run) {
    return page('Nightly', '<div class="banner empty">No nightly run recorded yet — first run lands at 03:00.</div>');
  }
  const reds = run.rows.filter((r) => r.status === 'fail');
  const skips = run.rows.filter((r) => r.status === 'skip');
  const greens = run.rows.filter((r) => r.status === 'pass');
  // A suite that reported neither pass, fail nor skip produced no parseable
  // output. It was silently absent from the counts, so 317 suites rendered as
  // 314 and three red-or-green-unknown suites read as nothing at all.
  const silent = run.rows.filter(
    (r) => !['pass', 'fail', 'skip'].includes(r.status),
  );
  const tests = tallyTests(run.rows);
  // #4063 — a run that has not completed has NO verdict yet. On 2026-09-02
  // 13:5x the 13:30 run was 13 suites in and the page bannered "ALL GREEN,
  // 12 passed / 13 total" (Silas): the green was computed over whichever
  // subset had reported — the vacuous-pass class. Partial = IN PROGRESS, in
  // amber, with "so far" on every count; green is only ever said of a whole
  // night.
  const { verdict, cls } = runVerdict(run, reds.length);
  const partial = notFinishedBanner(run);
  const row = (r: NightlyRow) => `
    <tr class="${esc(r.status)}">
      <td class="st">${esc(r.status)}</td>${labelCell(opts, r)}
      <td class="kind">${esc(r.kind)}</td>
      <td class="path">${esc(displayPath(r.path))}</td>
      <td>${esc(r.owner)}</td>
      <td class="sum">${esc(r.summary)}</td>
    </tr>`;
  const ordered = [...reds, ...silent, ...skips, ...greens];
  const body = `
  ${partial}
  <div class="banner ${cls}">
    <span class="verdict">${verdict}</span>
    <span class="counts">SUITES: ${greens.length} passed · ${reds.length} failed · ${skips.length} skipped${silent.length ? ' · ' + silent.length + ' produced no output' : ''} · ${run.rows.length} total</span>
    <span class="counts">TESTS: ${tests.passed} passed · ${tests.failed} failed${tests.unparsed ? ' (' + tests.unparsed + ' suite(s) report no test counts)' : ''}</span>
    <span class="when">${esc(run.startedAt)}${run.completedAt ? ' → ' + esc(run.completedAt) : ''}</span>
  </div>
  ${renderReadoutBanner(opts)}
  ${renderHistory(opts, run.startedAt)}
  <table>
    <thead><tr><th></th><th>means</th><th>tier</th><th>suite</th><th>owner</th><th>result</th></tr></thead>
    <tbody>${ordered.map(row).join('')}</tbody>
  </table>
  <p class="prov">cargo tier runs via <code>werk-test --nightly</code> — registry selection, nextest, typed needs-stack skips (#3920). Page renders the run record verbatim; it holds no verdict of its own.</p>`;
  return page(`Nightly — ${verdict}`, body);
}

function page(title: string, body: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>
  :root { --bg:#fff; --fg:#1a1a1a; --mut:#667; --red:#c0392b; --green:#1e7d32; --amber:#8a6d1a; --line:#e5e5ea; }
  @media (prefers-color-scheme: dark) { :root { --bg:#131316; --fg:#eee; --mut:#99a; --red:#ff6b5e; --green:#5dd879; --amber:#e3c05a; --line:#2a2a30; } }
  body { background:var(--bg); color:var(--fg); font:15px/1.5 -apple-system,system-ui,sans-serif; max-width:70rem; margin:2rem auto; padding:0 1rem; }
  .banner { padding:1rem 1.25rem; border-radius:10px; margin-bottom:1rem; display:flex; gap:1rem; align-items:baseline; flex-wrap:wrap; }
  .banner.green { background:color-mix(in srgb, var(--green) 12%, transparent); }
  .banner.red { background:color-mix(in srgb, var(--red) 12%, transparent); }
  .banner.partial, .banner.empty { background:color-mix(in srgb, var(--amber) 14%, transparent); }
  .banner.readout { background:color-mix(in srgb, var(--mut) 10%, transparent); flex-direction:column; gap:.25rem; }
  .banner.readout ul.delta { margin:.25rem 0 0; padding-left:1.25rem; }
  .banner.readout li.new { color:var(--red); } .banner.readout li.fixed { color:var(--green); }
  details.history { margin-bottom:1rem; color:var(--mut); }
  details.history ul { columns:2; padding-left:1.25rem; margin:.5rem 0 0; }
  details.history li.cur { font-weight:700; color:var(--fg); }
  .hl { font-size:.85rem; }
  .split b { color:var(--fg); }
  td.lbl { font-size:.75rem; font-weight:700; white-space:nowrap; }
  td.lbl.product-broke { color:var(--red); } td.lbl.test-wrong { color:var(--amber); } td.lbl.unmeasured { color:var(--mut); }
  .verdict { font-size:1.6rem; font-weight:700; }
  .banner.green .verdict { color:var(--green); } .banner.red .verdict { color:var(--red); }
  .counts, .when { color:var(--mut); }
  table { width:100%; border-collapse:collapse; }
  th { text-align:left; color:var(--mut); font-weight:600; padding:.4rem .5rem; border-bottom:1px solid var(--line); }
  td { padding:.35rem .5rem; border-bottom:1px solid var(--line); vertical-align:top; }
  tr.fail .st { color:var(--red); font-weight:700; }
  tr.pass .st { color:var(--green); }
  tr.skip .st { color:var(--amber); }
  .path { font-family:ui-monospace,monospace; font-size:.85rem; word-break:break-all; }
  .sum { color:var(--mut); }
  .prov { color:var(--mut); font-size:.85rem; margin-top:1.25rem; }
  code { font-family:ui-monospace,monospace; }
</style></head><body>${body}</body></html>`;
}
