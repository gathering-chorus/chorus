// @test-type: unit — pure functions over fixture documents. No fs, no server, no store.
//
// #4015 — Jeff, 2026-08-26: "this is my rendered surface and you can fetch the same
// data via api — api and ui match."
//
// So the report is ONE document with two renderings. The JSON is the report; the
// HTML is a view of it and holds no arithmetic of its own. If the page can compute
// a number the API does not return, the two can disagree — which is the defect this
// card exists to remove, relocated into the presentation layer.
//
// The cross-foot is what makes the document trustworthy, so it is what these proofs
// pin. Last night's real run is the fixture: 7,624 registered, 7,347 executed,
// 1,535 recorded — 5,812 results computed and thrown away while the run still
// printed a verdict.
import {
  crossFootChecks,
  reportVerdict,
  renderTestRun,
  renderStoredRun,
  buildTestRunReport,
  STORABLE_KINDS,
} from '../src/handlers/test-run-report';

const RECONCILED = {
  run: { id: 'nr-1', trigger: 'nightly', scope: 'full', startedAt: '2026-08-26T03:00:05', endedAt: '2026-08-26T03:36:40' },
  crossFoot: {
    registered: 100, selected: 100, notSelected: 0,
    executed: 90, notExecuted: 10,
    passed: 88, failed: 2, unmeasured: 0,
    storable: 90, recorded: 90, dropped: 0,
  },
  byKind: [{ kind: 'cargo', suites: 1, passed: 88, failed: 2, cases: 90, caseMeaning: 'one #[test] fn' }],
  cases: [],
  footer: { neverExecuted: [], failed: [], dropped: 0, changedSinceLastRun: [] },
};

// Last night, as it actually happened.
const REAL = {
  ...RECONCILED,
  crossFoot: {
    registered: 7624, selected: 7624, notSelected: 0,
    executed: 7347, notExecuted: 277,
    passed: 7089, failed: 258, unmeasured: 0,
    storable: 7347, recorded: 1535, dropped: 5812,
  },
};


// Only NUMERIC CELLS count. Scanning every digit in the page catches years inside
// timestamps ("2026-08-26"), which are the document's own strings, not figures the
// view computed — and a proof that fires on those would be noise, so nobody would
// keep it. The cells are where a fabricated number would actually appear.
function numericCells(html: string): number[] {
  const out: number[] = [];
  const re = /<td class="n">([\d,]+)<\/td>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(html)) !== null) out.push(Number(m[1].replace(/,/g, '')));
  return out;
}

describe('#4015 cross-foot — the report must add up or say it does not', () => {
  it('control: a reconciled document passes every check', () => {
    const checks = crossFootChecks(RECONCILED.crossFoot);
    expect(checks.every(c => c.ok)).toBe(true);
    expect(checks.length).toBeGreaterThanOrEqual(4);
  });

  it('negative proof: passed + failed + unmeasured must equal executed', () => {
    // The 2026-08-25 shape: a suite reported counts that did not sum to what ran.
    const bad = { ...RECONCILED.crossFoot, passed: 80 }; // 80 + 2 + 0 != 90
    const check = crossFootChecks(bad).find(c => c.name === 'executed');
    expect(check!.ok).toBe(false);
  });

  it('negative proof: results stored must equal results produced, and last night none were', () => {
    const check = crossFootChecks(REAL.crossFoot).find(c => c.name.startsWith('results stored'));
    expect(check!.ok).toBe(false);
    expect(REAL.crossFoot.dropped).toBe(5812);
  });

  it('control: results stored reconciles when nothing was dropped', () => {
    const check = crossFootChecks(RECONCILED.crossFoot).find(c => c.name.startsWith('results stored'));
    expect(check!.ok).toBe(true);
  });
});

describe('#4015 verdict — a broken report never reads green', () => {
  it('negative proof: dropped results suppress the verdict entirely', () => {
    // A run that loses four fifths of its results cannot tell anyone nothing is wrong.
    expect(reportVerdict(REAL)).toBe('RESULTS LOST');
  });

  it('control: a reconciled run with failures reads FAIL, not BROKEN', () => {
    expect(reportVerdict(RECONCILED)).toBe('FAIL');
  });

  it('control: a reconciled run with no failures reads PASS', () => {
    const clean = { ...RECONCILED, crossFoot: { ...RECONCILED.crossFoot, passed: 90, failed: 0 } };
    expect(reportVerdict(clean)).toBe('PASS');
  });
});

describe('#4015 api and ui match — the view holds no arithmetic', () => {
  it('every number the page prints appears in the document it was given', () => {
    // The load-bearing one. If the view can compute a figure the API does not
    // return, the two surfaces can drift and Jeff is back to reconciling by hand.
    const html = renderTestRun(REAL);
    const printed = numericCells(html);
    const inDoc = new Set<number>(Object.values(REAL.crossFoot) as number[]);
    for (const k of REAL.byKind) { inDoc.add(k.suites); inDoc.add(k.passed); inDoc.add(k.failed); inDoc.add(k.cases); }
    const orphans = printed.filter(n => !inDoc.has(n));
    expect(orphans).toEqual([]);
  });

  it('negative proof: a number the document does not carry is caught', () => {
    // Without this the check above could pass by printing nothing numeric at all.
    const html = renderTestRun(REAL) + '<td class="n">9,999,999</td>';
    const printed = numericCells(html);
    const inDoc = new Set<number>(Object.values(REAL.crossFoot) as number[]);
    for (const k of REAL.byKind) { inDoc.add(k.suites); inDoc.add(k.passed); inDoc.add(k.failed); inDoc.add(k.cases); }
    expect(printed.filter(n => !inDoc.has(n))).toContain(9999999);
  });

  it('the dropped line is rendered when there is a drop, and not when there is not', () => {
    // Key on the ROW, not the word — the cross-foot label now also contains
    // 'dropped' (Silas asked for it), and a proof that matches the label instead
    // of the row would pass while the row itself vanished.
    expect(renderTestRun(REAL)).toContain('dropped-row');
    expect(renderTestRun(RECONCILED)).not.toContain('dropped-row');
  });
});

describe('#4015 review fixes — unknown is not a pass and not a failure', () => {
  it('negative proof: an unverifiable check never reports ok', () => {
    // Silas and Wren both caught `selected` reading ✓ while it was true by
    // construction. It must not silently pass.
    const sel = crossFootChecks(RECONCILED.crossFoot).find(c => c.name.startsWith('selected'));
    expect(sel!.state).toBe('unknown');
  });

  it('control: an unknown check does NOT make the report BROKEN', () => {
    // The other half. If unknown forced BROKEN, every report would be broken
    // forever and the state would stop meaning anything.
    expect(reportVerdict(RECONCILED)).toBe('FAIL');
  });

  it('negative proof: a genuinely failing check still forces RESULTS LOST', () => {
    expect(reportVerdict(REAL)).toBe('RESULTS LOST');
  });

  it('the view renders ? for unknown, distinct from ✓ and ✗', () => {
    const html = renderTestRun(REAL);
    expect(html).toContain('?</td>');
    expect(html).toContain('✗</td>');
  });
});

describe('#4015 review fix — unmeasured plan fields are null, never fabricated', () => {
  it('negative proof: the built document carries null, not selected=registered', () => {
    // Wren: the checks said UNKNOWN but the raw JSON still carried a plausible
    // fabricated pair — authoritative-looking to any consumer reading fields.
    const doc = { ...RECONCILED, crossFoot: { ...RECONCILED.crossFoot, selected: null, notSelected: null } };
    expect(doc.crossFoot.selected).toBeNull();
    const html = renderTestRun(doc as never);
    expect(html).toContain('—');
    expect(html).not.toContain('>-1<');
  });

  it('negative proof: the built document carries notExecuted null, not a fabricated 0', () => {
    // Wren, second pass: notExecuted was hardcoded 0 at the builder call site —
    // the same fabricated-denominator shape, masked by scopeCheck being UNKNOWN.
    const log = 'RUN|start|2026-08-26T03:00:05\nSUITE|cargo|platform/x|kade|pass|2 pass, 0 fail\nRUN|complete|2026-08-26T03:36:40\n';
    const doc = buildTestRunReport({
      runId: 'r', trigger: 'nightly', scope: 'full',
      logText: log, registered: 2, recorded: 2, notExecuted: null,
    });
    expect(doc!.crossFoot.notExecuted).toBeNull();
    const scope = crossFootChecks(doc!.crossFoot).find(c => c.name.startsWith('scope'));
    expect(scope!.state).toBe('unknown');
    const html = renderTestRun(doc!);
    expect(html).not.toContain('>-1<');
  });
});


describe('#4015 demo finding — recorded measures against STORABLE, not executed', () => {
  it('negative proof: a perfect run with unstorable kinds still goes green', () => {
    // The 2026-08-27 10:25 run: shell assertions, probes and ratchets can never
    // be stored per-case. Before this fix the storage row was un-greenable by
    // construction — found by running a real credentialed nightly, not review.
    const cf = { ...RECONCILED.crossFoot, executed: 100, passed: 98, failed: 2,
                 storable: 60, recorded: 60, dropped: 0 };
    const check = crossFootChecks(cf).find(c => c.name.startsWith('results stored'));
    expect(check!.state).toBe('ok');
  });

  it('control: storable results that were NOT stored still fail the row', () => {
    const cf = { ...RECONCILED.crossFoot, storable: 60, recorded: 10, dropped: 50 };
    const check = crossFootChecks(cf).find(c => c.name.startsWith('results stored'));
    expect(check!.state).toBe('fail');
  });
});

describe('#4015 — the store-derived "most recent stored run" section', () => {
  it('renders the newest stored run from row counts alone', () => {
    // Jeff, 2026-08-27: "that makes no sense to show data from last night."
    // This section reads the store, so a run is visible the moment its rows land.
    const html = renderStoredRun({
      runTs: '2026-08-27T16:38:00-04:00', total: 218, passed: 211, failed: 7,
      byKind: [{ kind: 'npm', total: 160, passed: 158, failed: 2 },
               { kind: 'cargo', total: 58, passed: 53, failed: 5 }],
    });
    expect(html).toContain('2026-08-27T16:38:00-04:00');
    expect(html).toContain('218');
    expect(html).toContain('reads the store, not the log');
  });

  it('negative proof: an empty store yields a refusal, never an invented run', () => {
    const html = renderStoredRun(null);
    expect(html).toContain('will not invent one');
    expect(html).not.toContain('<table');
  });
});

describe('#4022 — security cases are storable (run 6, 2026-08-29)', () => {
  const LOG = [
    'RUN|start|2026-08-29T12:10:21|pid=87534',
    'SUITE|cargo|platform/services/werk-test|kade|pass|100 pass, 0 fail',
    'SUITE|security|platform/tests/3924-declared-wins.bats|silas|fail|0 pass, 2 fail',
    'SUITE|shell|platform/scripts/test-x.sh|silas|pass|5 pass, 0 fail',
    'RUN|complete|2026-08-29T13:09:31|suites=3',
  ].join('\n');

  it('negative proof: the run-6 shape — security stored but not counted as storable — reads as RESULTS LOST', () => {
    // What Jeff saw on his phone at 13:14: "stored 7,293 of 5,721 ✗". The runner
    // saves security cases (bats with registered identities); the page did not
    // count them, so stored outran its own denominator and the verdict lied.
    expect(STORABLE_KINDS.has('security')).toBe(true);
    expect(STORABLE_KINDS.has('shell')).toBe(false);
    const r = buildTestRunReport({ runId: 'nr', trigger: 'nightly', scope: 'full',
      logText: LOG, registered: 200, recorded: 102, notExecuted: null })!;
    expect(r.crossFoot.storable).toBe(102);   // 100 cargo + 2 security; shell's 5 excluded
    expect(reportVerdict(r)).not.toBe('RESULTS LOST');
  });

  it('control: storable cases that were NOT stored still read as lost', () => {
    const r = buildTestRunReport({ runId: 'nr', trigger: 'nightly', scope: 'full',
      logText: LOG, registered: 200, recorded: 100, notExecuted: null })!;
    expect(r.crossFoot.dropped).toBe(2);
    expect(reportVerdict(r)).toBe('RESULTS LOST');
  });
});

// #4030 AC4 — suites the run planned and never reached are RED on this page,
// listed by name, never silently absent. 2026-08-30 03:00: the runner was
// killed at its lane cap with five npm packages and every bats suite unrun;
// the report counted 3 red. nightly-suites.sh now folds each unreached unit
// into a `NEVER RAN` fail row; the document names them and fails.
describe('#4030 never-ran suites are red', () => {
  const HEAD = 'RUN|start|2026-08-30T03:00:00';
  const RAN = 'SUITE|cargo|platform/services/werk-test|silas|pass|191 pass, 0 fail';
  const NEVER = 'SUITE|npm|platform/api|silas|fail|0 pass, 1 fail (NEVER RAN — the runner was killed at the lane cap before this unit (rc=124))';
  const REAL_FAIL = 'SUITE|shell|platform/scripts/test-x.sh|silas|fail|0 pass, 1 fail (synthesized rc=1, no parseable line)';
  const TAIL = 'RUN|complete|2026-08-30T05:15:54';
  const build = (lines: string[]) => buildTestRunReport({
    runId: 'latest', trigger: 'nightly', scope: 'full selection',
    logText: lines.join('\n'), registered: 191, recorded: 191, notExecuted: null,
  })!;

  it('negative proof: a NEVER RAN row makes the verdict FAIL and is named in the footer', () => {
    const doc = build([HEAD, RAN, NEVER, TAIL]);
    expect(reportVerdict(doc)).toBe('FAIL');
    expect(doc.footer.neverExecuted).toEqual(['npm platform/api']);
    expect(doc.footer.failed).toEqual([]);
    const html = renderTestRun(doc);
    expect(html).toContain('never ran');
    expect(html).toContain('npm platform/api');
  });

  it('control: the same run without the row is PASS with nothing never-ran', () => {
    const doc = build([HEAD, RAN, TAIL]);
    expect(reportVerdict(doc)).toBe('PASS');
    expect(doc.footer.neverExecuted).toEqual([]);
    expect(renderTestRun(doc)).not.toContain('never ran');
  });

  it('a real one-assertion failure is failed, not never-ran — the marker decides, not the counts', () => {
    const doc = build([HEAD, RAN, REAL_FAIL, TAIL]);
    expect(reportVerdict(doc)).toBe('FAIL');
    expect(doc.footer.failed).toEqual(['shell platform/scripts/test-x.sh']);
    expect(doc.footer.neverExecuted).toEqual([]);
  });
});

// #4035 — a stopped run's block ends at RUN|stopped: its rows stay its own and
// endedAt is the stop time, so `recorded` binds to the right window.
describe('#4035 RUN|stopped ends the block', () => {
  it('endedAt is the stop time and later runs are not swallowed', () => {
    const doc = buildTestRunReport({
      runId: 'latest', trigger: 'nightly', scope: 'full selection',
      logText: [
        'RUN|start|2026-08-31T03:00:01',
        'SUITE|cargo|platform/services/werk-test|silas|pass|191 pass, 0 fail',
        'RUN|stopped|2026-08-31T03:21:44|signal=TERM pid=17545',
      ].join('\n'),
      registered: 191, recorded: 191, notExecuted: null,
    })!;
    expect(doc.run.endedAt).toBe('2026-08-31T03:21:44');
    expect(doc.byKind).toHaveLength(1);
  });
});
