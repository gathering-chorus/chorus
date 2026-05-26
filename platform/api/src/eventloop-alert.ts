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
//
// #3079 — current-op sentinel: scheduled jobs call setCurrentOp() before/after so the
// blocked callback captures which op was running. If op=unknown, the block fired outside
// all tracked jobs (request handler or untracked async). No async_hooks; zero overhead.

/** Set by scheduled jobs before running; cleared in finally. Captured by blocked callback. */
let _currentOp: string | null = null;

/** Called by each scheduled job: setCurrentOp('index') before, setCurrentOp(null) after. */
export function setCurrentOp(op: string | null): void {
  _currentOp = op;
}

/** Read by blocked callback at fire time. */
export function getCurrentOp(): string {
  return _currentOp ?? 'unknown';
}

/** Shape we need from Express's req/res — tiny + injectable so the middleware
 * is unit-testable without spinning up Express. */
export interface ReqLike { method: string; path: string }
export interface ResLike { once(event: 'finish' | 'close', listener: () => void): void }

/** #3089: Express middleware that names the request handler so block alerts
 * attribute to a route, not `op=unknown`. The #3079 sentinel only fires for
 * SCHEDULED jobs (setCurrentOp at the cron sites); without this middleware,
 * request-handler blocks log op=unknown → "No cause inferred" → hand-grep-and-guess.
 * Single-slot is correct for the common case: sync handlers serialize on the event
 * loop, so only one is on-loop at a time and `_currentOp` reflects it. Clearing on
 * `finish` AND `close` so the op doesn't leak past the response. Known limitation:
 * async-resume — req A awaits, B enters+sets, A resumes+blocks → attributed to B.
 * Full per-async-context attribution would need AsyncLocalStorage; deferred. */
export function makeRequestOpMiddleware(): (req: ReqLike, res: ResLike, next: () => void) => void {
  return (req: ReqLike, res: ResLike, next: () => void): void => {
    setCurrentOp(`${req.method} ${req.path}`);
    const clear = (): void => setCurrentOp(null);
    res.once('finish', clear);
    res.once('close', clear);
    next();
  };
}

export interface BlockAlert {
  duration_ms: number;
  ts: string;
  op: string;
  message: string;
}

/** Pure, honest formatter — only the measured block + which op was running (or unknown). */
export function formatBlockAlert(durationMs: number, ts: string, op: string): BlockAlert {
  const opNote = op === 'unknown'
    ? `The slow request is in the access log at this time — grep chorus-api.log around ${ts} for the route.`
    : `Captured op: ${op}.`;
  return {
    duration_ms: durationMs,
    ts,
    op,
    message:
      `chorus-api event loop blocked ${durationMs}ms at ${ts}. ` +
      opNote +
      ` No cause inferred; this is the measured block only.`,
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
      const a = formatBlockAlert(Math.round(ms), new Date(now()).toISOString(), getCurrentOp());
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
