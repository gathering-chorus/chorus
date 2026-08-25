// @test-type: unit — pure functions over a fixture log string; no fs, no server
//
// #4009 — the page must distinguish a run that is WORKING from one that is
// WEDGED. On 2026-08-25 both rendered as "PARTIAL", so a reader could not act:
// a lane sat silent 38 minutes while the same banner had been shown for a run
// that was healthy ten minutes earlier.
//
// Three states, three proofs — plus the controls, because a check that always
// says "quiet" separates nothing (#3734).
import { parseNightlyLog, quietVerdict, renderNightlyPage } from '../src/handlers/nightly-report';

const LOG = [
  'RUN|start|2026-08-25T17:44:52|pid=71067',
  'SUITE|lint|/x|kade|pass|1 pass, 0 fail',
  'SUITE|npm|/y|wren|fail|3 pass, 2 fail',
].join('\n');

const DONE = [LOG, 'RUN|complete|2026-08-25T18:27:00|suites=2'].join('\n');

describe('#4009 run liveness — working vs wedged', () => {
  it('a completed run is complete regardless of quiet time', () => {
    const run = parseNightlyLog(DONE)!;
    expect(quietVerdict(run, 60 * 60 * 1000)).toBe('complete');
  });

  it('control: a recently-writing run reads LIVE, not wedged', () => {
    const run = parseNightlyLog(LOG)!;
    expect(quietVerdict(run, 2 * 60 * 1000)).toBe('live');
  });

  it('negative proof: silence past the threshold reads QUIET (the state we could not see)', () => {
    const run = parseNightlyLog(LOG)!;
    expect(quietVerdict(run, 38 * 60 * 1000)).toBe('quiet');
  });

  it('the rendered page names the wedge in words a reader can act on', () => {
    const run = parseNightlyLog(LOG)!;
    run.quietForMs = 38 * 60 * 1000;
    const html = renderNightlyPage(run);
    expect(html).toContain('NO OUTPUT for 38 min');
    expect(html).toContain('wedged, not slow');
    expect(html).not.toContain('RUNNING —');
  });

  it('control: a live run renders as RUNNING with its progress, never as wedged', () => {
    const run = parseNightlyLog(LOG)!;
    run.quietForMs = 60 * 1000;
    const html = renderNightlyPage(run);
    expect(html).toContain('RUNNING —');
    expect(html).toContain('2 suite(s) so far');
    expect(html).not.toContain('NO OUTPUT');
  });
});
