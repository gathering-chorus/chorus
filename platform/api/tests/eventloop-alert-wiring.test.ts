// #3050 — wiring behavior: every block is recorded (spine), nudges are throttled
// (no spam on a hot loop), and the cold-start window is excluded (boot delay) so a
// deploy restart can't false-fire — the exact failure mode of the old shell alert.
import { startEventloopAlert, BlockAlert } from '../src/eventloop-alert';

describe('#3050 startEventloopAlert — detect, throttle, cold-start exclusion', () => {
  it('records every block, throttles nudges, and ignores the cold-start window', () => {
    jest.useFakeTimers();
    let cb: (ms: number) => void = () => {};
    const emits: BlockAlert[] = [];
    const nudges: BlockAlert[] = [];
    let clock = 1_000_000;

    startEventloopAlert({
      blockedFn: (fn) => { cb = fn; },
      emit: (a) => emits.push(a),
      nudge: (a) => nudges.push(a),
      bootDelayMs: 10_000,
      throttleMs: 300_000,
      now: () => clock,
    });

    // cold-start window: monitoring not wired yet, a block here is ignored
    cb(1500);
    expect(emits).toHaveLength(0);

    // after the boot delay, monitoring is live
    jest.advanceTimersByTime(10_000);
    cb(1500);                 // block 1
    clock += 1_000; cb(2500); // block 2, within the throttle window
    expect(emits).toHaveLength(2);          // every block recorded (witness)
    expect(emits[0].duration_ms).toBe(1500);
    expect(nudges).toHaveLength(1);         // throttled: only the first nudged

    clock += 300_000; cb(3000);             // block 3, past the throttle window
    expect(nudges).toHaveLength(2);         // nudges again

    jest.useRealTimers();
  });
});
