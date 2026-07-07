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

import { AsyncLocalStorage } from 'node:async_hooks';
import { boston } from './time-utils';

/** #3096 — Request-vs-scheduled boundary for the eventloop attribution surface:
 *  TWO surfaces, TWO slots, ONE reader.
 *
 *  Request handlers run inside an AsyncLocalStorage `run()` block. The op set
 *  by the middleware survives `await` resumption because Node tracks the
 *  originating async context — not whichever request most recently touched a
 *  global slot. This closes Class A (single-slot clobber by fast peers during
 *  slow A's await) and Class C (a /board/wip poll reading the heavy op of
 *  something behind it). A awaits → B runs in its OWN ALS context → A resumes
 *  → A's op is still the one captured. No more `op=unknown` from overlap.
 *
 *  Scheduled jobs (boardCache refresh, healthCache, future cron paths) don't
 *  have a request context, so they continue to set the module-level slot via
 *  setCurrentOp() before/after. The reader is ALS-first, slot-second; either
 *  surface fills it, neither steps on the other. */
const requestOpStore = new AsyncLocalStorage<{ op: string }>();

/** Module-level slot for scheduled-job paths (no request context). */
let _currentOp: string | null = null;

/** Called by each scheduled job: setCurrentOp('index') before, setCurrentOp(null) after.
 *  Request handlers do NOT use this — they get an ALS-bound op via the middleware. */
export function setCurrentOp(op: string | null): void {
  _currentOp = op;
}

/** Read by blocked callback at fire time. ALS first (per-request, async-safe),
 *  module slot second (scheduled jobs), 'unknown' as the honest fallback. */
export function getCurrentOp(): string {
  const fromAls = requestOpStore.getStore()?.op;
  if (fromAls) return fromAls;
  return _currentOp ?? 'unknown';
}

/** Shape we need from Express's req/res — tiny + injectable so the middleware
 * is unit-testable without spinning up Express. */
export interface ReqLike { method: string; path: string }
export interface ResLike { once(event: 'finish' | 'close', listener: () => void): void }

/** #3089 → #3096: Express middleware that names the request handler so block
 *  alerts attribute to a route, not `op=unknown`. The op is bound to the
 *  request's async context via AsyncLocalStorage: `als.run({ op }, next)`
 *  pins the value across every `await` the handler performs, so a slow A
 *  resuming after a fast B no longer reads B's op (Class A clobber) and a
 *  /board/wip poll heavy-behind no longer reads the heavy op (Class C
 *  coincidence). The store cleans itself up when the async tree completes —
 *  no `finish`/`close` listeners required, and the scheduled-job slot is
 *  untouched. */
export function makeRequestOpMiddleware(): (req: ReqLike, res: ResLike, next: () => void) => void {
  return (req: ReqLike, _res: ResLike, next: () => void): void => {
    requestOpStore.run({ op: `${req.method} ${req.path}` }, next);
  };
}

export interface BlockAlert {
  duration_ms: number;
  ts: string;
  op: string;
  message: string;
  /** #3610 — present only in captureStacks mode: the blocked-at stack of the
   *  resource that blocked. The top frame IS the call site — measured, not inferred. */
  stack?: string[];
}

/** Pure, honest formatter — only the measured block + which op was running (or unknown).
 *  `ts` field stays ISO (storage contract for spine event correlation); the
 *  human-facing `message` body renders Boston (#3093 — render-vs-storage
 *  boundary, Jeff doesn't read UTC). */
export function formatBlockAlert(durationMs: number, ts: string, op: string, stack?: string[]): BlockAlert {
  const display = boston(ts);
  const frames = (stack ?? []).map((f) => f.trim()).filter((f) => f.length > 0);
  const opNote = op === 'unknown'
    ? `The slow request is in the access log at this time — grep chorus-api.log around ${display} for the route.`
    : `Captured op: ${op}.`;
  // A captured stack replaces the correlate-it-yourself pointer: the frame is
  // the measured call site, still no inferred story beyond it.
  const causeNote = frames.length > 0 ? `Blocked at: ${frames[0]}.` : opNote;
  return {
    duration_ms: durationMs,
    ts,
    op,
    ...(frames.length > 0 ? { stack: frames } : {}),
    message:
      `chorus-api event loop blocked ${durationMs}ms at ${display}. ` +
      causeNote +
      ' No cause inferred; this is the measured block only.',
  };
}

export interface EventloopAlertDeps {
  /** the `blocked` library callback-registrar; injectable for tests */
  blockedFn?: (cb: (ms: number) => void, opts: { threshold: number }) => void;
  /** #3610 — the `blocked-at` registrar (stack-capturing); injectable for tests.
   *  Only consulted when captureStacks is true. */
  blockedAtFn?: (cb: (ms: number, stack: string[]) => void, opts: { threshold: number }) => void;
  /** #3610 — diagnostic mode: capture the blocking stack via blocked-at
   *  (async-hooks overhead — NEVER always-on; #3050's decision stands). Wired
   *  from CHORUS_EVENTLOOP_STACKS=1 for a bounded trace window, then turned off. */
  captureStacks?: boolean;
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
  let lastNudge = 0;

  const fire = (ms: number, stack?: string[]): void => {
    const a = formatBlockAlert(Math.round(ms), new Date(now()).toISOString(), getCurrentOp(), stack);
    deps.emit(a);
    if (now() - lastNudge >= throttleMs) {
      lastNudge = now();
      deps.nudge(a);
    }
  };

  const start = () => {
    if (deps.captureStacks) {
      // #3610 diagnostic mode — blocked-at names the call site. The library's
      // callback is (time, stack, {type, resource}); adapt to (ms, stack).
      const blockedAtFn = deps.blockedAtFn ?? ((cb: (ms: number, stack: string[]) => void, opts: { threshold: number }) => {

        const blockedAt = require('blocked-at');
        blockedAt((time: number, stack: string[]) => cb(time, stack), opts);
      });
      blockedAtFn((ms, stack) => fire(ms, stack), { threshold });
      return;
    }
    const blockedFn = deps.blockedFn ?? require('blocked');
    blockedFn((ms: number) => fire(ms), { threshold });
  };

  const t = setTimeout(start, bootDelayMs);
  if (typeof t.unref === 'function') t.unref();
}
