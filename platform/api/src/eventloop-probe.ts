// #3082 — external-probe eventloop detector.
//
// The in-process detector (eventloop-alert.ts, the `blocked` library) rides the
// very loop it measures: a hard block starves the detector's own timer, so it
// UNDER-reports its own blocks (the card's correctness bug — the monitor distorts
// what it monitors). This worker runs in a SEPARATE process and measures
// chorus-api's loop from OUTSIDE: it probes the API's response latency on an
// interval. The worker's own loop is idle, so when chorus-api's loop is blocked
// the probe is slow (or times out) and the measurement is accurate.
//
// Mirrors index-worker.ts (#3085): a testable `runEventloopProbe(deps)` core with
// injected probe/emit/nudge/clock, plus a thin entry block wiring real deps.
// Reuses formatBlockAlert so the off-loop alert is byte-identical to the in-process
// one (same spine contract, same human message) — only the vantage changes.

import { formatBlockAlert, BlockAlert } from './eventloop-alert';

export interface ProbeEval {
  latencyMs: number;
  threshold: number;
  ts: string;
  op: string;
  timedOut?: boolean;
}

/** Pure decision: a probe whose latency met/exceeded threshold (or timed out) IS a
 *  block. A timeout means the loop never answered within the window, so the block is
 *  at least the timeout duration. Returns the (reused) BlockAlert, or null when the
 *  loop answered fast. */
export function evaluateProbe(input: ProbeEval): BlockAlert | null {
  const blocked = input.timedOut === true || input.latencyMs >= input.threshold;
  if (!blocked) return null;
  const op = input.timedOut ? 'probe-timeout' : input.op;
  return formatBlockAlert(Math.round(input.latencyMs), input.ts, op);
}

export interface ProbeResult {
  latencyMs: number;
  timedOut: boolean;
}

export interface EventloopProbeDeps {
  /** measure chorus-api's response latency once (real: GET /health, time it) */
  probe: () => Promise<ProbeResult>;
  /** spine record — fires on EVERY block (witness, cheap, real data) */
  emit: (a: BlockAlert) => void;
  /** call-to-action delivery — throttled */
  nudge: (a: BlockAlert) => void;
  threshold?: number;     // ms; default 1000
  throttleMs?: number;    // min gap between nudges; default 300000 (5m)
  intervalMs?: number;    // gap between probes; default 2000
  now?: () => number;
  sleep?: (ms: number) => Promise<void>;
  /** bounded tick count for tests; omit to run forever (the real worker) */
  ticks?: number;
}

/** The probe loop: measure → on the RISING EDGE of a block (healthy→blocked) emit once
 *  + nudge (throttled); re-arm on recovery. Emits the same BlockAlert shape as the
 *  in-process detector, and the SAME once-per-block-episode semantics — a sustained or
 *  chronically-slow loop emits ONCE per episode, not once per 2s probe (#3082 Kade
 *  review: don't flood eventloop.blocked events the way the synthetic-flood did today).
 *  The only difference from the in-process detector is the vantage: this can't be
 *  starved by the block it's measuring. */
export async function runEventloopProbe(deps: EventloopProbeDeps): Promise<void> {
  const threshold = deps.threshold ?? 1000;
  const throttleMs = deps.throttleMs ?? 300_000;
  const intervalMs = deps.intervalMs ?? 2000;
  const now = deps.now ?? Date.now;
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const bounded = typeof deps.ticks === 'number';
  let lastNudge = Number.NEGATIVE_INFINITY;
  let inBlock = false; // edge state: are we inside an ongoing block episode?
  let t = 0;

  while (!bounded || t < (deps.ticks as number)) {
    const { latencyMs, timedOut } = await deps.probe();
    const a = evaluateProbe({
      latencyMs,
      threshold,
      ts: new Date(now()).toISOString(),
      op: 'probe',
      timedOut,
    });
    if (a) {
      if (!inBlock) {
        // rising edge: a NEW block episode — emit the witness once, nudge if past throttle.
        inBlock = true;
        deps.emit(a);
        if (now() - lastNudge >= throttleMs) {
          lastNudge = now();
          deps.nudge(a);
        }
      }
      // still blocked (inBlock): suppress — one episode, one alert. No flood.
    } else {
      inBlock = false; // recovered → re-arm for the next episode
    }
    await sleep(intervalMs);
    t++;
  }
}

// Entry: wire real deps. The probe is an HTTP GET to chorus-api's own health route
// with a timeout; emit + nudge are IDENTICAL to the in-process detector's wiring
// (server.ts ~3346) — same chorus-log `eventloop.blocked silas domain=chorus ...`
// spine event, same ops-nudge to silas, same 3000ms threshold — so retiring the
// in-process detector is a transparent cutover (same alert, only the vantage moves
// off the blocked loop). Launched by chorus-eventloop-probe-worker.sh as a persistent
// KeepAlive process (LaunchAgent registration routes through Silas, ADR-012).
if (require.main === module) {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { execFile } = require('node:child_process') as typeof import('node:child_process');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const path = require('node:path') as typeof import('node:path');
  const os = require('node:os') as typeof import('node:os');

  const apiBase = process.env.CHORUS_API_BASE || 'http://localhost:3340';
  const timeoutMs = Number(process.env.CHORUS_PROBE_TIMEOUT_MS || 8000);
  const root = process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects', 'chorus');
  const CHORUS_LOG = path.join(root, 'platform/scripts/chorus-log');
  const OPS_NUDGE = path.join(root, 'platform/scripts/ops-nudge');

  const probe = async (): Promise<ProbeResult> => {
    // #3082 Kade review: the probe op MUST be cheap + constant-cost so latency reads
    // pure loop-availability, not handler work — a heavy endpoint would conflate a
    // loop block with its own compute. /context/health is a cache READ (healthCache
    // .snapshot(), in-memory) — constant-cost; it only "slows" when the loop can't run
    // it, which is exactly the signal we want. Do NOT point this at an aggregating route.
    const start = Date.now();
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), timeoutMs);
    try {
      await fetch(`${apiBase}/api/chorus/context/health`, { signal: ctrl.signal });
      return { latencyMs: Date.now() - start, timedOut: false };
    } catch {
      return { latencyMs: timeoutMs, timedOut: true };
    } finally {
      clearTimeout(timer);
    }
  };

  void runEventloopProbe({
    probe,
    emit: (a) =>
      // #3082 (Silas review): detector=probe distinguishes this off-loop alert from the
      // in-process one during the activate-before-retire overlap — so double-alerts are
      // DIAGNOSABLE and the retire-verification is provable from the data (you SEE the
      // probe catching blocks), not inferred from the in-process detector's absence.
      execFile('bash', [CHORUS_LOG, 'eventloop.blocked', 'silas', 'domain=chorus',
        `duration_ms=${a.duration_ms}`, `ts=${a.ts}`, `op=${a.op}`, 'detector=probe'], () => {}),
    // #3407 — chorus-api is Wren's layer; route the event-loop-block ALERT to wren
    // (the spine-emit role above stays the chorus-api emitter context).
    nudge: (a) => execFile('bash', [OPS_NUDGE, 'wren', a.message], () => {}),
    threshold: 3000,
  });
}
