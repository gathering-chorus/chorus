/**
 * #3443 AC7 — werk run-state: make a chorus_werk transport drop a NON-EVENT.
 *
 * The drop root (server.ts:2011) is that executeChorusWerk `await`s the entire
 * multi-minute act run synchronously, so the MCP transport gives up before act
 * returns ("response for tool chorus_werk was lost") and the caller can't tell
 * whether anything happened. The fix has two halves:
 *
 *   1. act runs DETACHED; the verb writes a run-state record and returns
 *      immediately with {runId, phase:'running'} — nothing is held open.
 *   2. a re-invoke (or an explicit status query) READS the run-state instead of
 *      starting a second act: if a run is still active it ATTACHES (returns the
 *      live phase); once act finishes, the terminal phase + real result are on
 *      record. So a drop → re-invoke → the true state, never a lost call or a
 *      double-act.
 *
 * This module is the pure decision + reason-extraction core, unit-tested without
 * fs or a real act. The fs-backed read/write + the detached spawn wire on top.
 */

export type WerkRunPhase = 'running' | 'presented' | 'landed' | 'failed';

export interface WerkRun {
  runId: string;
  card: number;
  role: string;
  go: boolean;
  phase: WerkRunPhase;
  startedAt: string; // ISO; supplied by caller (no Date.now in pure core)
  pid?: number;
  /** On phase==='failed': the child verb's REAL reason, never "step=X exit=1". */
  failureReason?: string;
}

export type RunAction =
  | { kind: 'start' } // no live run for this card → spawn act, write run-state
  | { kind: 'attach'; run: WerkRun }; // a run is already live/terminal → return it, do NOT double-act

/**
 * Decide what a chorus_werk invocation should do given the card's existing
 * run-state. The whole point: a re-invoke after a transport drop must NOT start
 * a second act — it attaches to the existing run and reports its real phase.
 *
 *  - no record / null            → start (first run)
 *  - record exists, ANY phase    → attach (running → report live; terminal →
 *                                  report the recorded outcome). Re-invoking is
 *                                  idempotent: a drop never causes a double-act.
 *
 * `requestedGo` lets a GO re-invoke after a no-go present proceed: a terminal
 * `presented` run that the caller now asks to GO is the next legitimate phase
 * (the land), so it starts; everything else attaches.
 */
export function decideRunAction(existing: WerkRun | null, requestedGo: boolean): RunAction {
  if (!existing) return { kind: 'start' };
  // A GO after a presented stop is the next legitimate phase (the land), not a
  // duplicate — only when the prior run actually reached 'presented'.
  if (requestedGo && !existing.go && existing.phase === 'presented') {
    return { kind: 'start' };
  }
  // Everything else: attach to what's on record (no second act).
  return { kind: 'attach', run: existing };
}

/**
 * Extract the child verb's REAL failure reason from an act run's output, instead
 * of collapsing it to "step=X exit=1" (the orchestrator-swallows-child-reason
 * gap Kade hit: he only learned the truth by hand-running the verb binary).
 *
 * Precedence (most specific first):
 *   1. a verb's structured `{"reason":"..."}` (the typed refusal taxonomy)
 *   2. a `reason=<token>` field in the verb's own error line
 *   3. the last non-empty stderr line (the verb's actual message)
 *   4. fall back to the step name if nothing richer is present
 */
export function extractFailureReason(stdout: string, stderr: string, step: string): string {
  const combined = `${stdout}\n${stderr}`;
  const jsonReason = combined.match(/"reason"\s*:\s*"([^"]+)"/);
  if (jsonReason) return jsonReason[1];
  const kvReason = combined.match(/\breason=([^\s,"]+)/);
  if (kvReason) return kvReason[1];
  const stderrLines = stderr.split('\n').map((l) => l.trim()).filter(Boolean);
  if (stderrLines.length) return stderrLines[stderrLines.length - 1].slice(0, 300);
  return step ? `step=${step} (no child reason surfaced)` : 'unknown';
}
