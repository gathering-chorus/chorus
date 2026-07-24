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
  /** #3678 — stamped at the running→presented transition (announce-repeat guard). */
  presentedAt?: string;
  /** #3678 — the PRIOR run's present, carried at launch so a re-presented same
   *  patch inside the window trips the demo.announce.repeated spine warning. */
  prevPresentedAt?: string;
  prevPatchId?: string;
  /** #3538 — the werk's patch-id (git patch-id of merge-base(origin/main,HEAD)..HEAD)
   *  at the time this run was recorded. A re-invoke compares it to the werk's CURRENT
   *  patch-id: if HEAD advanced (different patch), a 'presented' record is stale and
   *  the new commit must re-demo. Sibling of #3461's gather-gate patch-keying. */
  patchId?: string;
  /** #3664 — THIS run's own log file (runId-keyed). Before this, every start truncated
   *  the shared per-card log, so a relaunch destroyed the failed run's evidence (the
   *  #3660 unrecoverable-reason defect). Reconcile reads the record's own log; absent
   *  (pre-#3664 records) falls back to the legacy per-card path. */
  logFile?: string;
}

export type RunAction =
  | { kind: 'start' } // no live run for this card → spawn act, write run-state
  | { kind: 'attach'; run: WerkRun } // a run is already live/terminal → return it, do NOT double-act
  // #3678 AC2 — go while the pipeline is mid-flight: the accepter's go can only
  // attach to a PRESENTED round they actually saw. Queueing it onto whatever
  // presents next would land unseen work (the 2026-07-23 #3592 near-miss).
  | { kind: 'refuse-go-running'; run: WerkRun };

/**
 * #3638 — is the recorded patch-id superseded by the werk's current one?
 * The #3538 comparison, hardened for the #3421 stuck-present: a record persisted
 * with an EMPTY patchId could never trip headChanged, so a fix commit after the
 * present attached to the stale present forever (the only escape was hand-deleting
 * the run record). Now an empty/absent RECORDED key on a KNOWN current key reads as
 * superseded — one re-demo re-keys the record, then polls attach normally. An
 * unknown CURRENT key (git hiccup at poll time) still degrades to not-superseded
 * (attach), never a spurious re-run.
 */
export function patchSuperseded(recordedPatchId: string | undefined, currentPatchId: string): boolean {
  if (!currentPatchId) return false;
  return (recordedPatchId ?? '') !== currentPatchId;
}

/**
 * Decide what a chorus_werk invocation should do given the card's existing
 * run-state. The whole point: a re-invoke after a transport drop must NOT start
 * a second act — it attaches to the existing run and reports its real phase.
 *
 *  - no record / null            → start (first run)
 *  - record phase==='failed'     → start (RETRY — see below)
 *  - record phase 'running'/'presented'/'landed' → attach (running → report live;
 *                                  terminal → report the recorded outcome).
 *                                  Re-invoking is idempotent: a drop never
 *                                  double-acts.
 *
 * `requestedGo` lets a GO re-invoke after a no-go present proceed: a terminal
 * `presented` run that the caller now asks to GO is the next legitimate phase
 * (the land), so it starts; everything else attaches.
 *
 * `headChanged` (#3538) lets a NEW COMMIT after a present re-demo: a 'presented'
 * record whose patch-id is superseded (the werk's current HEAD advanced past the
 * recorded patch) is stale, so the new commit starts a fresh run. Computed by the
 * impure caller (current werk patch-id vs the recorded one). The sibling of #3461:
 * #3461 keys the gather-gate by patch so replies SURVIVE a content-identical churn;
 * this RE-TRIGGERS when the patch actually CHANGES. A same-patch rebase
 * (headChanged=false) still attaches — no needless re-run.
 */
/** The go-specific decisions, split out for complexity budget (#3678). */
function goDecision(existing: WerkRun, isStale: boolean): RunAction | null {
  // #3678 AC2 — a go against a LIVE running record refuses, typed. (A STALE
  // running record falls through to the stale→start branch: retrying a dead
  // run is legitimate; silently attaching a go to a live one is not.)
  if (existing.phase === 'running' && !isStale) {
    return { kind: 'refuse-go-running', run: existing };
  }
  // A GO after a presented stop is the next legitimate phase (the land).
  if (!existing.go && existing.phase === 'presented') return { kind: 'start' };
  return null;
}

export function decideRunAction(
  existing: WerkRun | null,
  requestedGo: boolean,
  isStale = false,
  headChanged = false,
  requestedRepresent = false,
): RunAction {
  if (!existing) return { kind: 'start' };
  if (requestedGo) {
    const g = goDecision(existing, isStale);
    if (g) return g;
  }
  // #3458 — a stale 'running' (dead pid/TTL) is retried, never attached forever.
  if (existing.phase === 'running' && isStale) return { kind: 'start' };
  // #3443 — a terminal 'failed' run is retryable; attaching forever strands the card.
  if (existing.phase === 'failed') return { kind: 'start' };
  // #3538 — a HUMAN commit after the present re-demos (headChanged); #3678 AC3 —
  // represent is the caller's explicit fresh-round request. Everything else is a
  // status check: read-only, attaches.
  if (existing.phase === 'presented' && (headChanged || requestedRepresent)) {
    return { kind: 'start' };
  }
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
/** #3664 (Kade gather) — cap on a stored failureReason: keeps the run record a small
 *  readable pointer (full detail lives in the run's own log), not a log dump. */
export const FAILURE_REASON_MAX = 300;

export function extractFailureReason(stdout: string, stderr: string, step: string): string {
  const combined = `${stdout}\n${stderr}`;
  const jsonReason = combined.match(/"reason"\s*:\s*"([^"]+)"/);
  if (jsonReason) return jsonReason[1];
  const kvReason = combined.match(/\breason=([^\s,"]+)/);
  if (kvReason) return kvReason[1];
  const stderrLines = stderr.split('\n').map((l) => l.trim()).filter(Boolean);
  if (stderrLines.length) return stderrLines[stderrLines.length - 1].slice(0, FAILURE_REASON_MAX);
  return step ? `step=${step} (no child reason surfaced)` : 'unknown';
}

/**
 * #3458 — the detached act run appends `WERK_EXIT=<code>` to its per-card log when
 * it finishes (the durable terminal marker that survives an mcp-server restart,
 * since it's on disk, not in-process). parseExitSentinel reads it at poll time:
 *
 *   - number → act is DONE (0 = presented/landed, non-zero = failed)
 *   - null   → no sentinel yet (still running, or crashed before writing it — the
 *              isRunStale pid/TTL backstop covers the crash case)
 *
 * Last match wins (a reused log only ever has one real run, but be defensive).
 */
/**
 * #3664 (Silas gather) — structured HELD sentinel, parallel to WERK_EXIT. The act
 * run's outcome step writes `WERK_HELD=<reason>` explicitly when a GO was given
 * but the witness did not prove (merge/deploy/accept were SKIPPED, job exits 0).
 * Matching free-form `[HELD]` log text was fragile coupling to GHA output format;
 * this is a deliberate machine contract. null → not held. Last match wins.
 */
export function parseHeldSentinel(logContent: string): string | null {
  const matches = logContent.match(/^WERK_HELD=(.*)$/gm);
  if (!matches || matches.length === 0) return null;
  const last = matches[matches.length - 1];
  return last.slice('WERK_HELD='.length).trim() || 'held: GO given but demo not proven';
}

export function parseExitSentinel(logContent: string): number | null {
  const matches = logContent.match(/WERK_EXIT=(\d+)/g);
  if (!matches || matches.length === 0) return null;
  const last = matches[matches.length - 1];
  const n = Number(last.slice('WERK_EXIT='.length));
  return Number.isNaN(n) ? null : n;
}


/**
 * #3678 AC4 — the loop becomes the SYSTEM's finding: a round presenting the
 * SAME patch again within the window means the pipeline announced twice for
 * one piece of work (2026-07-23: three demo-ready nudges at Jeff in 25 min).
 * Pure; the caller supplies now + window and emits the spine warning.
 */
export function announceRepeated(
  prev: { presentedAt?: string; patchId?: string } | null,
  nowIso: string,
  newPatchId: string,
  windowMs: number,
): boolean {
  if (!prev?.presentedAt || !prev.patchId) return false;
  if (prev.patchId !== newPatchId) return false;
  const prevMs = Date.parse(prev.presentedAt);
  const nowMs = Date.parse(nowIso);
  if (Number.isNaN(prevMs) || Number.isNaN(nowMs)) return false;
  return nowMs - prevMs <= windowMs;
}
