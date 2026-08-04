// @test-type: unit — injected deps, fake timers, no live services
// #3750 AC1 — the watchdog must NEVER take down the process it watches.
// 2026-08-04 10:42: require('blocked-at') threw inside start() and wedged
// chorus-api half-dead (port open, nothing answering). NEGATIVE PROOF (#3734):
// each fixture below drives the exact violated state — a throwing
// instrumentation dep / a throwing periodic callback — and asserts the
// process-visible behavior is DEGRADE (log + disable), never a throw.
import { startEventloopAlert, BlockAlert } from '../src/eventloop-alert';

describe('#3750 watchdog degrades instead of crashing', () => {
  afterEach(() => {
    jest.useRealTimers();
    jest.restoreAllMocks();
  });

  it('captureStacks registrar throwing (the 10:42 shape) disables the watchdog, no throw', () => {
    jest.useFakeTimers();
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    startEventloopAlert({
      captureStacks: true,
      blockedAtFn: () => { throw new Error("Cannot find module 'blocked-at'"); },
      emit: () => { throw new Error('emit must not be reached'); },
      nudge: () => { throw new Error('nudge must not be reached'); },
      bootDelayMs: 1000,
    });
    // The registrar throws when the boot-delay timer fires — this advance is
    // the crash moment; it must not propagate.
    expect(() => jest.advanceTimersByTime(1500)).not.toThrow();
    expect(err.mock.calls.some(c => String(c[0]).includes('eventloop-alert DISABLED'))).toBe(true);
  });

  it('plain blocked registrar throwing also degrades', () => {
    jest.useFakeTimers();
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    startEventloopAlert({
      blockedFn: () => { throw new Error("Cannot find module 'blocked'"); },
      emit: () => {},
      nudge: () => {},
      bootDelayMs: 1000,
    });
    expect(() => jest.advanceTimersByTime(1500)).not.toThrow();
    expect(err.mock.calls.some(c => String(c[0]).includes('eventloop-alert DISABLED'))).toBe(true);
  });

  it('a throw inside the periodic fire callback degrades and the watchdog keeps running', () => {
    jest.useFakeTimers();
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    let cb: (ms: number) => void = () => {};
    const emits: BlockAlert[] = [];
    let emitCalls = 0;
    startEventloopAlert({
      blockedFn: (fn) => { cb = fn; },
      emit: (a) => { emitCalls++; if (emitCalls === 1) throw new Error('spine write exploded'); emits.push(a); },
      nudge: () => {},
      bootDelayMs: 1000,
      throttleMs: 0,
      now: () => 10_000_000 + emitCalls,
    });
    jest.advanceTimersByTime(1500);
    expect(() => cb(2000)).not.toThrow();      // first fire: emit throws → degraded
    expect(err.mock.calls.some(c => String(c[0]).includes('fire failed'))).toBe(true);
    expect(() => cb(3000)).not.toThrow();      // watchdog still alive: second fire works
    expect(emits).toHaveLength(1);
  });
});
