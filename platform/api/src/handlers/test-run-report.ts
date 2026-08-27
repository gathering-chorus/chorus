// #4015 — the test-run report: ONE document, two renderings.
//
// Jeff, 2026-08-26: "this is my rendered surface and you can fetch the same data
// via api — api and ui match."
//
// So the JSON below IS the report. `renderTestRun` is a view of it and computes
// nothing: every figure it prints must already be in the document. That rule is
// enforced by a test, not by discipline, because the alternative is two surfaces
// that can quietly disagree — the exact defect this card exists to remove, moved
// into the presentation layer.
//
// The wireframe Jeff accepted:
// https://claude.ai/code/artifact/4697dd0b-5e3c-4090-ac7e-22bbc9ba4ade

/** `registered` is null when the tests domain could not be asked. A cross-foot
 *  against a fabricated denominator is worse than no cross-foot, so the checks
 *  that need it report UNKNOWN rather than compare against 0 (Silas, #4015). */
export interface CrossFoot {
  registered: number;
  /** null = the runner has not reported its plan. Wren, reviewing #4015: the
   *  checks marked these UNKNOWN but the raw JSON still carried fabricated
   *  selected=registered / notSelected=0, which look authoritative to any
   *  consumer that reads fields instead of checks. Unmeasured is null, not a
   *  plausible default — the same rule that makes the route 503 instead of
   *  substituting 0 for a store that did not answer. */
  selected: number | null;
  notSelected: number | null;
  executed: number;
  /** null = the runner has not reported its plan, so "how many were not
   *  executed" is unmeasurable — same rule as `selected` (Wren, #4015: a
   *  hardcoded 0 here is the fabricated-denominator shape this card removes). */
  notExecuted: number | null;
  passed: number;
  failed: number;
  unmeasured: number;
  /** cases from per-case-capable lanes only — the honest denominator for `recorded` */
  storable: number;
  recorded: number;
  dropped: number;
}

/** Which kinds can produce per-case stored results at all. Shell suites carry
 *  counts only, security probes and ratchets are checks not cases — comparing
 *  `recorded` against a total that includes them makes the storage row
 *  structurally un-greenable. Found by RUNNING the report against a real
 *  credentialed nightly on 2026-08-27, not by review. */
export const STORABLE_KINDS = new Set(['cargo', 'npm', 'jest', 'bats']);

export interface KindTotal {
  kind: string;
  suites: number;
  passed: number;
  failed: number;
  cases: number;
  /** What one case means for this kind. A kind with no per-case names says so
   *  here rather than inventing rows (shell suites carry counts only). */
  caseMeaning: string;
}

export interface TestRunReport {
  run: {
    id: string; trigger: string; scope: string;
    card?: string; role?: string;
    startedAt: string; endedAt: string; durationMs?: number;
  };
  crossFoot: CrossFoot;
  byKind: KindTotal[];
  cases: Array<{ kind: string; filePath: string; testName: string; result: string; durationMs?: number; source?: string }>;
  footer: { neverExecuted: string[]; failed: string[]; dropped: number; changedSinceLastRun: string[] };
}

/** Three states, not two. UNKNOWN is what a check reports when the input it needs
 *  was never supplied — distinct from a check that ran and disagreed. Collapsing
 *  them either hides a real break (unknown read as ok) or cries wolf forever
 *  (unknown read as fail), and a check that always fails gets ignored like one
 *  that always passes. */
export interface Check { name: string; lhs: number; rhs: number; ok: boolean; state: 'ok' | 'fail' | 'unknown' }


const eq = (name: string, lhs: number, rhs: number): Check =>
  ({ name, lhs, rhs, ok: lhs === rhs, state: lhs === rhs ? 'ok' : 'fail' });

/** `selected` can only be verified against a plan the runner reports. Until it
 *  does, this is UNKNOWN — never a silent ✓. */
const scopeCheck = (c: CrossFoot): Check => ({
  name: 'scope (plan not reported — cannot verify)',
  lhs: c.notExecuted === null ? -1 : c.executed + c.notExecuted, rhs: c.selected ?? -1,
  ok: true, state: 'unknown',
});

const selectedCheck = (c: CrossFoot): Check => ({
  name: 'selected (plan not reported — cannot verify)',
  lhs: c.selected ?? -1, rhs: c.registered - (c.notSelected ?? 0),
  ok: true, state: 'unknown',
});

/** Four checks: two verified equations (executed, results stored) and two that
 *  are UNKNOWN until the runner reports its plan (selected, scope). Every one is
 *  stated in the JSON so the page can show its working instead of asserting a
 *  total — and the UNKNOWN pair is named as unverified, not counted as green. */
export function crossFootChecks(c: CrossFoot): Check[] {
  return [
    // #4015 review (Silas, Wren): this read as a tautology while the builder set
    // selected=registered and notSelected=0 — a check that cannot fail (#3734).
    // The run's plan is not in the log, so it reports UNKNOWN until the runner
    // says what it planned. Unknown is not a pass and not a failure.
    selectedCheck(c),
    // Same honesty as `selected`. Silas and Wren both showed this firing
    // BROKEN REPORT on every real run because notExecuted was hardcoded 0 —
    // conflating "we did not run everything" (a coverage gap) with "we lost the
    // evidence" (an integrity failure). Different states, different owners.
    // UNKNOWN until the runner reports what it planned.
    scopeCheck(c),
    eq('executed', c.passed + c.failed + c.unmeasured, c.executed),
    // Silas, reviewing #4015: this row demands MORE than a balance — dropped must be
    // zero. Named so, because 'recorded' reading ✓ while results were lost is the
    // exact false comfort this document exists to remove.
    // Jeff, reading the live page: "recorded 7,411 | 7,411 | ✗" — two identical
    // numbers marked wrong, because the row displayed recorded+dropped (equal to
    // executed BY CONSTRUCTION) while failing on dropped>0. Show the comparison
    // the row is actually making: results stored vs results the run COULD store.
    // vs STORABLE, not executed — shell assertions, probes and ratchets carry no
    // per-case identity, so the executed denominator made this row un-greenable
    // by construction. Found by running a real credentialed nightly (2026-08-27).
    eq('results stored (of storable)', c.recorded, c.storable),
  ];
}

/** A run that lost results cannot report on itself, so this outranks PASS and
 *  FAIL. It is named for what happened to THE RUN, not for the page: Jeff read
 *  "BROKEN REPORT" in red at the top and concluded the page was broken, which is
 *  the only fair reading of those words. The verdict describes the nightly. */
export function reportVerdict(r: TestRunReport): 'PASS' | 'FAIL' | 'RESULTS LOST' {
  if (crossFootChecks(r.crossFoot).some(c => c.state === 'fail')) return 'RESULTS LOST';
  return r.crossFoot.failed > 0 ? 'FAIL' : 'PASS';
}


/** Minimal styling for the rendered view. Presentation only — it carries no
 *  numbers, which is what keeps "api and ui match" true by construction. */
export const TEST_RUN_CSS = `<style>
:root{--paper:#fbfbfc;--ink:#16181d;--muted:#626a78;--line:#e3e5ea;--pass:#1c7c34;--fail:#c0392b}
@media(prefers-color-scheme:dark){:root{--paper:#121317;--ink:#e9eaee;--muted:#9aa2b1;--line:#292b33;--pass:#5dd879;--fail:#ff7568}}
body{background:var(--paper);color:var(--ink);font:15px/1.55 -apple-system,system-ui,sans-serif;max-width:64rem;margin:0 auto;padding:2rem 1rem}
h1{font-size:1.3rem;margin:0 0 .3rem}
.verdict{font:600 1.6rem/1.2 ui-monospace,monospace;margin:.2rem 0 1rem}
.verdict.ok{color:var(--pass)}.verdict.bad{color:var(--fail)}
table{width:100%;border-collapse:collapse;font-size:.88rem;margin:1.2rem 0}
th{text-align:left;font-size:.72rem;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);padding:.35rem .5rem;border-bottom:1px solid var(--line)}
td{padding:.32rem .5rem;border-bottom:1px solid var(--line)}
td.n,th.n{text-align:right;font-family:ui-monospace,monospace;font-variant-numeric:tabular-nums}
tr.bad td{color:var(--fail)}tr.ok td:last-child{color:var(--pass)}
</style>`;

// A switch, not a keyed lookup: eslint's security/detect-object-injection fires on
// `map[ch]` even when `ch` comes from the regex itself, and adopting a baseline
// bump to silence a lint on MY new code is how baselines stop meaning anything.
const esc = (s: string): string => s.replace(/[&<>"]/g, ch => {
  switch (ch) {
    case '&': return '&amp;';
    case '<': return '&lt;';
    case '>': return '&gt;';
    case '"': return '&quot;';
    default: return ch;
  }
});
const n = (v: number) => v.toLocaleString('en-US');

/** The view. Prints only what the document carries — no arithmetic here. */
export function renderTestRun(r: TestRunReport): string {
  const v = reportVerdict(r);
  const checks = crossFootChecks(r.crossFoot);
  const c = r.crossFoot;

  const foot = checks.map(k =>
    `<tr class="${k.state === 'ok' ? 'ok' : k.state === 'fail' ? 'bad' : 'unk'}"><td>${esc(k.name)}</td>` +
    // an UNKNOWN row's figures were never measured — an em dash, not a sentinel
    `<td class="n">${k.state === 'unknown' ? '—' : n(k.lhs)}</td><td class="n">${k.state === 'unknown' ? '—' : n(k.rhs)}</td>` +
    `<td>${k.state === 'ok' ? '✓' : k.state === 'fail' ? '✗' : '?'}</td></tr>`).join('');

  // Jeff: the cases column summed to 7,409 while `executed` said 7,411 — the two
  // unmeasured suites were counted in the total and rendered nowhere, so no reader
  // could reproduce the figure from the rows above it. A total you cannot add up
  // from what is on the page is the same defect as a total that is wrong.
  const unmeasuredRow = c.unmeasured > 0
    ? `<tr><td>unmeasured</td><td class="n">—</td><td class="n">—</td><td class="n">—</td>`
      + `<td class="n">${n(c.unmeasured)}</td><td>suite produced no counts — ran, said nothing</td></tr>`
    : '';
  const totalRow = `<tr class="total"><td><strong>total</strong></td><td class="n">—</td>`
    + `<td class="n">${n(c.passed)}</td><td class="n">${n(c.failed)}</td>`
    + `<td class="n">${n(c.executed)}</td><td></td></tr>`;
  const kinds = r.byKind.map(k =>
    `<tr><td>${esc(k.kind)}</td><td class="n">${n(k.suites)}</td>` +
    `<td class="n">${n(k.passed)}</td><td class="n">${n(k.failed)}</td>` +
    `<td class="n">${n(k.cases)}</td><td>${esc(k.caseMeaning)}</td></tr>`).join('');

  // The dropped line renders ONLY when something was dropped — its presence is
  // the signal, so it must never be a permanent row that readers learn to skip.
  const droppedRow = c.dropped > 0
    ? `<tr class="bad dropped-row"><td>dropped</td><td class="n">${n(c.dropped)}</td>`
      + '<td>the run computed a verdict for each of these and saved none of them, '
      + 'so nothing here can say whether the code is healthy</td></tr>'
    : '';

  return `<h1>Test run ${esc(r.run.id)}</h1>
<p class="verdict ${v === 'PASS' ? 'ok' : 'bad'}">${v}</p>
<p>${esc(r.run.trigger)} · ${esc(r.run.scope)} · ${esc(r.run.startedAt)} → ${esc(r.run.endedAt)}</p>
<table class="crossfoot"><tr><th>check</th><th class="n">is</th><th class="n">should be</th><th></th></tr>${foot}</table>
<table class="kinds"><tr><th>kind</th><th class="n">suites</th><th class="n">passed</th><th class="n">failed</th><th class="n">cases</th><th>a case is</th></tr>${kinds}${unmeasuredRow}${totalRow}</table>
<table class="footer">${droppedRow}</table>`;
}

/** The newest run group actually IN the store, regardless of trigger — every
 *  figure here is a count of saved rows, so this section can never show a run
 *  whose evidence was lost. Exists because the nightly-log view above showed
 *  Jeff yesterday's run after a day of runs whose results never reached the
 *  log (2026-08-27). */
export interface StoredRun {
  runTs: string;
  total: number;
  passed: number;
  failed: number;
  byKind: Array<{ kind: string; total: number; passed: number; failed: number }>;
}

export function renderStoredRun(sr: StoredRun | null): string {
  if (!sr) {
    return '<h2 class="stored">Most recent stored run</h2>'
      + '<p>the tests domain holds no stored runs — nothing to show, and this page will not invent one</p>';
  }
  const rows = sr.byKind.map(k =>
    `<tr><td>${esc(k.kind)}</td><td class="n">${n(k.passed)}</td>` +
    `<td class="n">${n(k.failed)}</td><td class="n">${n(k.total)}</td></tr>`).join('');
  return `<h2 class="stored">Most recent stored run — ${esc(sr.runTs)}</h2>
<p>every row counted here is a result row saved in the model at that timestamp — this section reads the store, not the log, so a run that saved nothing cannot appear on it</p>
<table class="storedrun"><tr><th>kind</th><th class="n">passed</th><th class="n">failed</th><th class="n">stored</th></tr>${rows}
<tr class="total"><td><strong>total</strong></td><td class="n">${n(sr.passed)}</td><td class="n">${n(sr.failed)}</td><td class="n">${n(sr.total)}</td></tr></table>`;
}

// ── building the document from a real run ──────────────────────────────────────

/** One SUITE row as the nightly log writes it. */
interface SuiteRow { kind: string; path: string; owner: string; status: string; summary: string }

/** What a case means, per kind — stated in the document so the page never has to
 *  know. A kind that cannot name individual cases says so here instead of
 *  inventing rows (shell suites carry counts only). */
const CASE_MEANING: Record<string, string> = {
  cargo: 'one #[test] fn',
  npm: 'one it()',
  bats: 'one @test',
  shell: 'one assertion in a .sh — this kind carries no per-case names',
  security: 'one probe against one route',
  coverage: 'one package measured against its floor',
  lint: 'one ratchet against its baseline',
  'app-eslint': 'one ratchet against its baseline',
  'coverage-denominator': 'crates carrying a coverage floor',
  smoke: 'one live-stack probe',
  ui: 'one playwright flow',
  reconcile: 'the registered-vs-executed census',
};

/** Pull the LAST complete run block out of the nightly log. */
export function lastRunSuites(text: string): { startedAt: string; endedAt?: string; rows: SuiteRow[] } | null {
  const all = text.split('\n');
  const start = all.reduce((acc, l, i) => (l.startsWith('RUN|start|') ? i : acc), -1);
  if (start === -1) return null;
  const out: SuiteRow[] = [];
  let endedAt: string | undefined;
  for (const l of all.slice(start + 1)) {
    if (l.startsWith('RUN|complete|')) { endedAt = l.split('|')[2]; break; }
    if (!l.startsWith('SUITE|')) continue;
    const [, kind, path, owner, status, ...rest] = l.split('|');
    if (rest.length >= 1) out.push({ kind, path, owner, status, summary: rest.join('|') });
  }
  const [, , startedAt] = all.slice(start, start + 1).join('').split('|');
  return { startedAt: startedAt || '?', endedAt, rows: out };
}

/** Fold the suite rows into per-kind totals. Counts come from each row's own
 *  summary; a row that produced no parseable counts contributes to `unmeasured`
 *  rather than silently reading as a clean zero (#4009). */
export function foldByKind(rows: SuiteRow[]): { byKind: KindTotal[]; unmeasured: number } {
  const acc = new Map<string, KindTotal>();
  let unmeasured = 0;
  for (const r of rows) {
    const m = r.summary.match(/(\d+) pass, (\d+) fail/);
    const k = acc.get(r.kind) ?? { kind: r.kind, suites: 0, passed: 0, failed: 0, cases: 0, caseMeaning: CASE_MEANING[r.kind] ?? 'one check' };
    k.suites += 1;
    if (m) {
      k.passed += Number(m[1]);
      k.failed += Number(m[2]);
      k.cases += Number(m[1]) + Number(m[2]);
    }
    if (r.status === 'unmeasured' || (m && m[1] === '0' && m[2] === '0')) unmeasured += 1;
    acc.set(r.kind, k);
  }
  return { byKind: [...acc.values()].sort((a, b) => b.cases - a.cases), unmeasured };
}

/** Assemble the document. `registered` and `recorded` come from the tests domain;
 *  everything else from the run's own suite rows. `dropped` is not a judgement —
 *  it is executed minus recorded, and it is the number that made the 2026-08-26
 *  run unreportable. */
export function buildTestRunReport(input: {
  runId: string; trigger: string; scope: string; card?: string; role?: string;
  logText: string; registered: number; recorded: number; notExecuted: number | null;
}): TestRunReport | null {
  const last = lastRunSuites(input.logText);
  if (!last) return null;
  const { byKind, unmeasured } = foldByKind(last.rows);
  const storable = byKind.filter(k => STORABLE_KINDS.has(k.kind))
    .reduce((s2, k) => s2 + k.cases, 0);
  // #4009 — a suite that produced no counts is UNMEASURED, and the document says
  // so; it must never be folded into `passed` where it reads as a clean zero.
  const passed = byKind.reduce((s, k) => s + k.passed, 0);
  const failed = byKind.reduce((s, k) => s + k.failed, 0);
  const executed = passed + failed + unmeasured;
  return {
    run: {
      id: input.runId, trigger: input.trigger, scope: input.scope,
      card: input.card, role: input.role,
      startedAt: last.startedAt, endedAt: last.endedAt ?? '',
    },
    crossFoot: {
      registered: input.registered,
      // Not measured — the runner does not report its plan yet. null, never a
      // fabricated registered/0 pair (Wren's review catch).
      selected: null,
      notSelected: null,
      executed,
      notExecuted: input.notExecuted,
      passed, failed, unmeasured,
      recorded: input.recorded,
      storable,
      dropped: Math.max(0, storable - input.recorded),
    },
    byKind,
    cases: [],
    footer: {
      neverExecuted: [],
      failed: last.rows.filter(r => r.status === 'fail').map(r => `${r.kind} ${r.path}`),
      dropped: Math.max(0, storable - input.recorded),
      changedSinceLastRun: [],
    },
  };
}
