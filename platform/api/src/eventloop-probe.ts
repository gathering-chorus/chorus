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

/** The probe loop: measure → if blocked, emit (always) + nudge (throttled) → wait.
 *  Emits the same BlockAlert shape as the in-process detector; the only difference
 *  is it can't be starved by the block it's measuring. */
export async function runEventloopProbe(deps: EventloopProbeDeps): Promise<void> {
  const threshold = deps.threshold ?? 1000;
  const throttleMs = deps.throttleMs ?? 300_000;
  const intervalMs = deps.intervalMs ?? 2000;
  const now = deps.now ?? Date.now;
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const bounded = typeof deps.ticks === 'number';
  let lastNudge = Number.NEGATIVE_INFINITY;
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
      deps.emit(a);
      if (now() - lastNudge >= throttleMs) {
        lastNudge = now();
        deps.nudge(a);
      }
    }
    await sleep(intervalMs);
    t++;
  }
}

// Entry: wire real deps. The probe is an HTTP GET to chorus-api's own health route
// with a timeout; emit appends a spine record; nudge posts to the messaging API.
// Launched by chorus-eventloop-probe-worker.sh on a LaunchAgent cadence (registration
// routes through Silas, per the card + ADR-012). Retiring the in-process detector in
// server.ts is the cutover step that follows this worker landing.
if (require.main === module) {
  const apiBase = process.env.CHORUS_API_BASE || 'http://localhost:3340';
  const timeoutMs = Number(process.env.CHORUS_PROBE_TIMEOUT_MS || 8000);

  const probe = async (): Promise<ProbeResult> => {
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
    emit: (a) => console.log(JSON.stringify({ ...a, source: 'eventloop-probe' })),
    nudge: (a) => console.error(`[eventloop-probe] ${a.message}`),
  });
}
