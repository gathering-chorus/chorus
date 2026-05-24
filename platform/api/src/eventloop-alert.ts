// #3050 — event-loop block alert, built on the `blocked` npm library (tj/node-blocked),
// replacing the hand-rolled proving/domains/alerts/chorus-api-eventloop-lag.yml shell alert.
//
// The day's lessons, encoded:
// - Use the proven library, not a hand-rolled watchdog/inspector framework.
// - Report ONLY what's measured (duration + time). No fabricated causal story — the
//   route is correlated from the access log by timestamp, not asserted here.
// - Don't false-fire on a deploy cold-start: monitoring starts after a boot delay.
// - `blocked` (duration only, lightweight, prod-safe), NOT `blocked-at` (async-hooks
//   overhead) — never put always-on async-hooks overhead on the coordination spine.

export interface BlockAlert {
  duration_ms: number;
  ts: string;
  message: string;
}

/** Pure, honest formatter — only the measured block, with a pointer (not a claim)
 *  at where the slow request lives. No cause is inferred. */
export function formatBlockAlert(durationMs: number, ts: string): BlockAlert {
  return {
    duration_ms: durationMs,
    ts,
    message:
      `chorus-api event loop blocked ${durationMs}ms at ${ts}. ` +
      `The slow request is in the access log at this time — grep chorus-api.log around ${ts} for the route. ` +
      `No cause inferred; this is the measured block only.`,
  };
}

export interface EventloopAlertDeps {
  /** the `blocked` library callback-registrar; injectable for tests */
  blockedFn?: (cb: (ms: number) => void, opts: { threshold: number }) => void;
  /** spine record — fires on EVERY block (witness, cheap, real data) */
  emit: (a: BlockAlert) => void;
  /** call-to-action delivery — throttled */
  nudge: (a: BlockAlert) => void;
  threshold?: number;     // ms; default 1000
  bootDelayMs?: number;   // delay before monitoring, excludes cold-start; default 10000
  throttleMs?: number;    // min gap between nudges; default 300000 (5m)
  now?: () => number;
}

/** Wire `blocked` (after a boot delay) so a deploy cold-start can't false-fire.
 *  Every block emits a real spine record; nudges are throttled so a hot loop
 *  doesn't spam. Returns nothing; idempotent only by caller discipline (call once). */
export function startEventloopAlert(deps: EventloopAlertDeps): void {
  const threshold = deps.threshold ?? 1000;
  const bootDelayMs = deps.bootDelayMs ?? 10_000;
  const throttleMs = deps.throttleMs ?? 300_000;
  const now = deps.now ?? Date.now;
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const blockedFn = deps.blockedFn ?? require('blocked');
  let lastNudge = 0;

  const start = () => {
    blockedFn((ms: number) => {
      const a = formatBlockAlert(Math.round(ms), new Date(now()).toISOString());
      deps.emit(a);
      if (now() - lastNudge >= throttleMs) {
        lastNudge = now();
        deps.nudge(a);
      }
    }, { threshold });
  };

  const t = setTimeout(start, bootDelayMs);
  if (typeof t.unref === 'function') t.unref();
}
