// @test-type: unit
// #3610 — blocked-at diagnostic mode: the AC needs the trace to NAME the exact
// blocking call site, and timestamp correlation can't do that (today's three
// windows were all confounded by machine-wide OOM thrash, and the scheduled-job
// op slot mislabels async spans). Stack capture is flag-gated and OFF by
// default — #3050's decision stands: no always-on async-hooks overhead on the
// coordination spine. This mode exists to be turned on for a bounded trace
// window, read, and turned back off.
import { startEventloopAlert, formatBlockAlert, BlockAlert } from '../src/eventloop-alert';

const TS = '2026-07-07T18:00:00.000Z';

describe('#3610 formatBlockAlert with a captured stack', () => {
  const STACK = [
    'at parseCardsListOutput (/api/src/board-cache.ts:69:22)',
    'at refresh (/api/src/board-cache.ts:103:15)',
  ];

  it('names the top frame in the human message and carries the full stack', () => {
    const a = formatBlockAlert(3500, TS, 'boardCache', STACK);
    expect(a.stack).toEqual(STACK);
    expect(a.message).toContain('Blocked at: at parseCardsListOutput (/api/src/board-cache.ts:69:22)');
    // the stack IS the call site — no access-log pointer needed
    expect(a.message.toLowerCase()).not.toContain('access log');
  });

  it('still reports the measured block honestly (duration + Boston time)', () => {
    const a = formatBlockAlert(3500, TS, 'unknown', STACK);
    expect(a.duration_ms).toBe(3500);
    expect(a.message).toContain('2026-07-07 14:00:00 EDT');
  });

  it('empty stack falls back to the stackless contract (no fabricated frame)', () => {
    const a = formatBlockAlert(3500, TS, 'unknown', []);
    expect(a.stack).toBeUndefined();
    expect(a.message.toLowerCase()).toContain('access log');
    expect(a.message).not.toContain('Blocked at:');
  });

  it('no stack argument → identical to the pre-#3610 message (regression guard)', () => {
    const withOp = formatBlockAlert(1234, TS, 'boardCache');
    expect(withOp.stack).toBeUndefined();
    expect(withOp.message.toLowerCase()).toContain('captured op: boardcache');
  });
});

describe('#3610 startEventloopAlert — captureStacks selects blocked-at', () => {
  it('captureStacks=true wires blockedAtFn and alerts carry the stack', () => {
    jest.useFakeTimers();
    let cb: (ms: number, stack: string[]) => void = () => {};
    const emits: BlockAlert[] = [];
    const blockedCalls: number[] = [];

    startEventloopAlert({
      captureStacks: true,
      blockedAtFn: (fn) => { cb = fn; },
      blockedFn: () => { blockedCalls.push(1); },
      emit: (a) => emits.push(a),
      nudge: () => {},
      bootDelayMs: 10_000,
      now: () => 1_000_000,
    });

    jest.advanceTimersByTime(10_000);
    cb(4200, ['at slowSync (/api/src/handlers/chorus-crawl.ts:242:20)']);

    expect(blockedCalls).toHaveLength(0); // blocked (duration-only) NOT wired
    expect(emits).toHaveLength(1);
    expect(emits[0].stack).toEqual(['at slowSync (/api/src/handlers/chorus-crawl.ts:242:20)']);
    expect(emits[0].message).toContain('Blocked at: at slowSync');
    jest.useRealTimers();
  });

  it('default (captureStacks unset) still uses blocked — zero async-hooks overhead', () => {
    jest.useFakeTimers();
    let cb: (ms: number) => void = () => {};
    const emits: BlockAlert[] = [];
    const blockedAtCalls: number[] = [];

    startEventloopAlert({
      blockedFn: (fn) => { cb = fn; },
      blockedAtFn: () => { blockedAtCalls.push(1); },
      emit: (a) => emits.push(a),
      nudge: () => {},
      bootDelayMs: 10_000,
      now: () => 1_000_000,
    });

    jest.advanceTimersByTime(10_000);
    cb(1500);

    expect(blockedAtCalls).toHaveLength(0);
    expect(emits).toHaveLength(1);
    expect(emits[0].stack).toBeUndefined();
    jest.useRealTimers();
  });
});
