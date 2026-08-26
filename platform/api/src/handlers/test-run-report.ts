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

export interface CrossFoot {
  registered: number;
  selected: number;
  notSelected: number;
  executed: number;
  notExecuted: number;
  passed: number;
  failed: number;
  unmeasured: number;
  recorded: number;
  dropped: number;
}

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

export interface Check { name: string; lhs: number; rhs: number; ok: boolean }

/** The four equations that make the document trustworthy. Every one is stated in
 *  the JSON so the page can show its working instead of asserting a total. */
export function crossFootChecks(c: CrossFoot): Check[] {
  return [
    { name: 'selected', lhs: c.selected, rhs: c.registered - c.notSelected, ok: c.selected === c.registered - c.notSelected },
    { name: 'scope', lhs: c.executed + c.notExecuted, rhs: c.selected, ok: c.executed + c.notExecuted === c.selected },
    { name: 'executed', lhs: c.passed + c.failed + c.unmeasured, rhs: c.executed, ok: c.passed + c.failed + c.unmeasured === c.executed },
    { name: 'recorded', lhs: c.recorded + c.dropped, rhs: c.executed, ok: c.recorded + c.dropped === c.executed && c.dropped === 0 },
  ];
}

/** A run that lost results cannot report on itself. BROKEN REPORT outranks both
 *  PASS and FAIL: on 2026-08-26 the 03:00 run computed 7,347 verdicts, stored
 *  1,535, and still printed a verdict — that must be impossible to read as green. */
export function reportVerdict(r: TestRunReport): 'PASS' | 'FAIL' | 'BROKEN REPORT' {
  if (crossFootChecks(r.crossFoot).some(c => !c.ok)) return 'BROKEN REPORT';
  return r.crossFoot.failed > 0 ? 'FAIL' : 'PASS';
}

const esc = (s: string) => s.replace(/[&<>"]/g, ch =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[ch] as string));
const n = (v: number) => v.toLocaleString('en-US');

/** The view. Prints only what the document carries — no arithmetic here. */
export function renderTestRun(r: TestRunReport): string {
  const v = reportVerdict(r);
  const checks = crossFootChecks(r.crossFoot);
  const c = r.crossFoot;

  const foot = checks.map(k =>
    `<tr class="${k.ok ? 'ok' : 'bad'}"><td>${esc(k.name)}</td>` +
    `<td class="n">${n(k.lhs)}</td><td class="n">${n(k.rhs)}</td>` +
    `<td>${k.ok ? '✓' : '✗'}</td></tr>`).join('');

  const kinds = r.byKind.map(k =>
    `<tr><td>${esc(k.kind)}</td><td class="n">${n(k.suites)}</td>` +
    `<td class="n">${n(k.passed)}</td><td class="n">${n(k.failed)}</td>` +
    `<td class="n">${n(k.cases)}</td><td>${esc(k.caseMeaning)}</td></tr>`).join('');

  // The dropped line renders ONLY when something was dropped — its presence is
  // the signal, so it must never be a permanent row that readers learn to skip.
  const droppedRow = c.dropped > 0
    ? `<tr class="bad"><td>dropped</td><td class="n">${n(c.dropped)}</td>` +
      `<td>executed, verdict computed, never stored — this report is missing ` +
      `${n(c.dropped)} of its own results</td></tr>`
    : '';

  return `<h1>Test run ${esc(r.run.id)}</h1>
<p class="verdict ${v === 'PASS' ? 'ok' : 'bad'}">${v}</p>
<p>${esc(r.run.trigger)} · ${esc(r.run.scope)} · ${esc(r.run.startedAt)} → ${esc(r.run.endedAt)}</p>
<table class="crossfoot"><tr><th>check</th><th class="n">is</th><th class="n">should be</th><th></th></tr>${foot}</table>
<table class="kinds"><tr><th>kind</th><th class="n">suites</th><th class="n">passed</th><th class="n">failed</th><th class="n">cases</th><th>a case is</th></tr>${kinds}</table>
<table class="footer">${droppedRow}</table>`;
}
