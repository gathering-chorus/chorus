// @test-type: unit — signal:api is fixture-data (pure functions over log fixtures)
// #4073 — every red carries one of three labels, DERIVED from the recorded run
// history, never declared. Jeff, 2026-09-02: "our tests seem very brittle ·
// its like whackamole · i never asked u to test mood". A red must say whether
// the product broke, the test is wrong, or nothing was measured.
import { parseAllRuns, buildReadout, renderReadoutText, labelRed } from '../src/handlers/nightly-readout';
import { renderNightlyPage } from '../src/handlers/nightly-report';

const run = (id: string, rows: string[]) =>
  [`RUN|start|${id}|pid=1`, ...rows, `RUN|complete|${id.replace('T03', 'T04')}|suites=${rows.length}`].join('\n');
const red = (s: string) => `SUITE|bats|${s}|silas|fail|bats: 0 passed, 1 failed`;
const green = (s: string) => `SUITE|bats|${s}|silas|pass|bats: 1 passed, 0 failed`;

describe('labelRed — the three states, from history alone', () => {
  it('PRODUCT BROKE: red every run since it last passed, no flipping', () => {
    // green, green, red, red, red  → a break that has held
    expect(labelRed(['pass', 'pass', 'fail', 'fail', 'fail'], '')).toBe('product-broke');
  });

  it('PRODUCT BROKE: a first red after a steady green history (a break since the last run)', () => {
    expect(labelRed(['pass', 'pass', 'pass', 'pass', 'fail'], '')).toBe('product-broke');
  });

  it('TEST WRONG: flips pass/fail across runs with no code change in between', () => {
    expect(labelRed(['pass', 'fail', 'pass', 'fail', 'pass', 'fail'], '')).toBe('test-wrong');
    // NEGATIVE PROOF (#3734): a suite red EVERY run must never be called test-wrong
    expect(labelRed(['fail', 'fail', 'fail', 'fail', 'fail'], '')).not.toBe('test-wrong');
  });

  it('TEST WRONG: a chronic flapper is test-wrong even when its last ten runs look like a fresh break', () => {
    const chronic = ['fail', 'pass', 'fail', 'pass', 'fail', 'pass', ...Array(9).fill('pass'), 'fail'];
    expect(labelRed(chronic, '')).toBe('test-wrong');
    // NEGATIVE PROOF: one old flip and a long steady green is a real break
    expect(labelRed(['fail', 'pass', ...Array(9).fill('pass'), 'fail'], '')).toBe('product-broke');
  });

  it('TEST WRONG: the failure names the machine, not the product', () => {
    for (const s of ['bats: 0 passed, 1 failed (latency 812ms > 500ms)', 'jest: timeout exceeded under load', 'ECONNREFUSED 127.0.0.1:3470', 'no live stack']) {
      expect(labelRed(['pass', 'pass', 'fail'], s)).toBe('test-wrong');
    }
  });

  it('UNMEASURED: the runner never took a reading', () => {
    expect(labelRed(['pass', 'fail'], '0 pass, 0 fail (runner produced no unit results rc=1)')).toBe('unmeasured');
    expect(labelRed(['pass', 'fail'], '0 pass, 1 fail (515 registered test(s) never ran of 7747 — 5xx in units that produced no results this run (platform/api 512 [killed after 1200s]))')).toBe('unmeasured');
    // #4063 — NEGATIVE PROOF: the census counted, no unit died: that is a red,
    // never "unmeasured" (the label that hid the row for four mornings)
    expect(labelRed(['fail', 'fail', 'fail'], '0 pass, 1 fail (289 registered test(s) never ran of 7800 — no unit of this run failed empty, so these are registered names the runners never emit — the ledger does not cross-foot)')).toBe('product-broke');
    expect(labelRed(['pass', 'fail'], '0 pass, 0 fail (UNMEASURED — suite produced no parseable output)')).toBe('unmeasured');
  });

  it('NEGATIVE PROOF: a plain first red with no history is product-broke, never silently unmeasured', () => {
    expect(labelRed(['fail'], 'bats: 0 passed, 3 failed')).toBe('product-broke');
  });
});

describe('the readout carries the labels and counts by them', () => {
  const LOG = [
    run('2026-09-01T03:00:00', [green('a.bats'), red('flappy.bats'), green('c.bats'), red('broken.bats')]),
    run('2026-09-02T03:00:00', [green('a.bats'), green('flappy.bats'), green('c.bats'), red('broken.bats')]),
    run('2026-09-03T03:00:00', [red('a.bats'), red('flappy.bats'), green('c.bats'), red('broken.bats'),
      'SUITE|reconcile|tests-domain|kade|fail|0 pass, 1 fail (55 registered test(s) never ran of 700 — 55 in units that produced no results this run (platform/api 55 [killed after 1200s]), 0 unattributed)']),
  ].join('\n') + '\n';

  it('each red names its label; the headline counts by label', () => {
    const runs = parseAllRuns(LOG);
    const r = buildReadout(runs[2], runs[1], runs);
    const byName = Object.fromEntries(r.reds.map((x) => [x.suite, x.label]));
    expect(byName['flappy.bats']).toBe('test-wrong');
    expect(byName['broken.bats']).toBe('product-broke');
    expect(byName['a.bats']).toBe('product-broke');
    expect(byName['tests-domain']).toBe('unmeasured');
    expect(r.byLabel).toEqual({ 'product-broke': 2, 'test-wrong': 1, unmeasured: 1 });
    const text = renderReadoutText(r, 'http://x');
    expect(text).toContain('4 red: 2 product broke, 1 test wrong, 1 unmeasured');
    expect(text).toMatch(/PRODUCT BROKE\s+silas\s+broken\.bats/);
    expect(text).toMatch(/TEST WRONG\s+silas\s+flappy\.bats/);
  });

  it('NEGATIVE PROOF: with a single run and no history, no red is labelled test-wrong', () => {
    const runs = parseAllRuns(run('2026-09-03T03:00:00', [red('a.bats'), red('b.bats')]) + '\n');
    const r = buildReadout(runs[0], null, runs);
    expect(r.reds.every((x) => x.label !== 'test-wrong')).toBe(true);
  });
});

describe('/nightly page carries the split', () => {
  it('shows the split line and a label on every red row, none on green rows', () => {
    const LOG = [
      run('2026-09-01T03:00:00', [green('a.bats'), red('flappy.bats'), red('broken.bats')]),
      run('2026-09-02T03:00:00', [green('a.bats'), green('flappy.bats'), red('broken.bats')]),
      run('2026-09-03T03:00:00', [green('a.bats'), red('flappy.bats'), red('broken.bats')]),
    ].join('\n') + '\n';
    const runs = parseAllRuns(LOG);
    const html = renderNightlyPage(runs[2], { readout: buildReadout(runs[2], runs[1], runs), history: runs });
    expect(html).toContain('2 red:</b> 1 product broke · 1 test wrong · 0 unmeasured');
    expect(html).toMatch(/td class="lbl test-wrong">TEST WRONG<\/td>/);
    expect(html).toMatch(/td class="lbl product-broke">PRODUCT BROKE<\/td>/);
    // NEGATIVE PROOF: a green row carries no label
    expect((html.match(/td class="lbl"><\/td>/g) || []).length).toBe(1);
  });
});

describe('a partial run drops nothing', () => {
  it('NEGATIVE PROOF: suites the run has not reached yet are not "no longer run"', () => {
    const LOG = run('2026-09-02T13:30:00', [green('a.bats'), green('b.bats'), green('c.bats')]) +
      '\nRUN|start|2026-09-02T15:38:49|pid=2\n' + green('a.bats') + '\n';
    const runs = parseAllRuns(LOG);
    const r = buildReadout(runs[1], runs[0], runs);
    expect(r.completed).toBe(false);
    expect(r.changes.gone).toEqual([]);
  });
});
