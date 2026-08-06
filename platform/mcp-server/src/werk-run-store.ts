/**
 * #3443 AC7 — fs persistence for werk run-state. The pure decision core
 * (werk-run-state.ts) decides start-vs-attach; this is the durable record it
 * decides against: one JSON file per card under a runs dir, written when act
 * starts (phase 'running'), advanced to the terminal phase when act finishes.
 *
 * A re-invoke after a transport drop reads this file and attaches to the live
 * run instead of starting a second act. Best-effort + crash-safe: a missing or
 * malformed file reads as null (→ the decision core treats it as "no run", so
 * the worst case degrades to today's start-fresh behavior, never a throw).
 */
import { readFileSync, writeFileSync, mkdirSync, existsSync, rmSync, renameSync, statSync } from 'fs';
import { execFileSync } from 'child_process';
import path from 'path';
import os from 'os';
import { parseExitSentinel, parseHeldSentinel, extractFailureReason, FAILURE_REASON_MAX, type WerkRun, type WerkRunPhase } from './werk-run-state';

export const RUNS_DIR = path.join(os.homedir(), '.chorus', 'werk-runs');

/** #3538 — the werk's current patch-id: `git patch-id --stable` of
 *  merge-base(origin/main,HEAD)..HEAD, mirroring werk-demo's git_patch_id. Computed
 *  with two execFileSync calls piped via stdin (no shell → no injection surface).
 *
 *  #3638 never-empty contract: a werk whose git works ALWAYS yields a key. When
 *  there is no diff vs main (resumed/landed werk) or patch-id itself fails, fall
 *  back to `sha:<HEAD>` — stricter than a patch-id (a rebase re-demos) but never
 *  the unkeyable '' that left #3421's present permanently stuck. '' now means
 *  only total git failure (not a repo), which callers degrade to attach. */
export function currentWerkPatchId(werkDir: string): string {
  const headKey = (): string => {
    try {
      const head = execFileSync('git', ['-C', werkDir, 'rev-parse', 'HEAD'], {
        encoding: 'utf8',
      }).trim();
      return head ? `sha:${head}` : '';
    } catch {
      return '';
    }
  };
  try {
    const base = execFileSync('git', ['-C', werkDir, 'merge-base', 'origin/main', 'HEAD'], {
      encoding: 'utf8',
    }).trim();
    if (!base) return headKey();
    const diff = execFileSync('git', ['-C', werkDir, 'diff', `${base}..HEAD`], {
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    });
    if (!diff) return headKey(); // no diff vs main → key on HEAD, not ''
    const out = execFileSync('git', ['patch-id', '--stable'], { input: diff, encoding: 'utf8' });
    return out.trim().split(/\s+/)[0] || headKey();
  } catch {
    return headKey();
  }
}

/** #3484 (Silas disposition) — the only variable filename component is `card`.
 *  Assert it's a positive integer so no `/` or `..` can ever reach path.join:
 *  a real guard backing the security/detect-non-literal-fs-filename disables. */
function assertCardId(card: number): void {
  if (!Number.isInteger(card) || card <= 0) {
    throw new Error(`werk-run-store: unsafe card id ${card}`);
  }
}

function runPath(dir: string, card: number): string {
  assertCardId(card);
  return path.join(dir, `${card}.json`);
}

/** Read the run record for a card, or null (missing/malformed → null, never throws). */
export function readRun(card: number, dir: string = RUNS_DIR): WerkRun | null {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is RUNS_DIR + `${card}.json`; card asserted positive-int (assertCardId), zero string interpolation → no traversal
    const raw = readFileSync(runPath(dir, card), 'utf8');
    const obj = JSON.parse(raw) as WerkRun | null;
    if (obj && typeof obj.card === 'number' && typeof obj.phase === 'string') return obj;
    return null;
  } catch {
    return null;
  }
}

/** Write/overwrite the run record. Best-effort (a write failure must not break the verb). */
export function writeRun(run: WerkRun, dir: string = RUNS_DIR): void {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- dir is the constant RUNS_DIR (test-injected dir only in unit tests)
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is RUNS_DIR + `${card}.json`; card asserted positive-int (assertCardId), zero string interpolation → no traversal
    writeFileSync(runPath(dir, run.card), JSON.stringify(run, null, 2));
  } catch {
    /* best-effort: a lost record degrades to start-fresh, never throws */
  }
}

/** Advance a run to a new phase (+ optional failureReason/pid), preserving identity. */
export function markPhase(
  card: number,
  phase: WerkRunPhase,
  extra: Partial<Pick<WerkRun, 'failureReason' | 'pid'>> = {},
  dir: string = RUNS_DIR,
): WerkRun | null {
  const cur = readRun(card, dir);
  if (!cur) return null;
  const next: WerkRun = { ...cur, phase, ...extra };
  writeRun(next, dir);
  return next;
}

/** Clear a card's run record (e.g. on accept/close so the next card starts clean). */
export function clearRun(card: number, dir: string = RUNS_DIR): void {
  try {
    rmSync(runPath(dir, card), { force: true });
  } catch {
    /* best-effort */
  }
}

/** #3751 — supersede a pin WITHOUT destroying it: rename `<card>.json` to
 *  `<card>.json.<tag>` so the record survives as forensics (the manual remedy on
 *  #3606 archived `.landed-*` / `.stale-*` files the same way; readRun keys on the
 *  exact `<card>.json` name, so archives are invisible to every reader).
 *  Returns the archive path, or null if there was nothing to archive / rename failed. */
export function archiveRun(card: number, tag: string, dir: string = RUNS_DIR): string | null {
  const src = runPath(dir, card);
  const safe = tag.replace(/[^A-Za-z0-9._-]/g, '_');
  const dest = `${src}.${safe}`;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- src is RUNS_DIR + `${card}.json` (card asserted positive-int); dest appends a sanitized tag
    if (!existsSync(src)) return null;
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- same controlled src/dest as the existsSync above (#3766: the disable covers one line only)
    renameSync(src, dest);
    return dest;
  } catch {
    return null;
  }
}

/** #3751 — is this pin from THIS card cycle, and is a 'landed' claim real?
 *
 *  The defect: card reopen (Done → Next/WIP) had no run-pin lifecycle, so a new
 *  cycle inherited the previous cycle's terminal record. chorus_werk attached to
 *  a JULY pin and answered {phase:'landed', accepter:'jeff'} for #3606 while the
 *  branch sat 10 commits ahead of origin/main with nothing merged — a false land
 *  report carrying the human's name, caught only by hand-checking git.
 *
 *  Two independent violations, checked only when the werk worktree EXISTS
 *  (a genuine land tears the werk down, so landed-with-no-werk is the normal
 *  terminal state and is not judged here):
 *
 *  1. pin-predates-cycle — the worktree's `.git` file is written once, at
 *     `git worktree add` (the pull that starts the cycle). A pin whose startedAt
 *     precedes it was written against a PREVIOUS worktree of the same card id.
 *  2. landed-but-unmerged — phase 'landed' with commits still ahead of
 *     origin/main is a lie regardless of age: a land merges by definition.
 *
 *  Uncertainty degrades to ok (missing stat/git → no refusal): this guard must
 *  only fire on positive evidence of a violation, never brick polling. */
export type PinIntegrityVerdict =
  | { ok: true }
  | { ok: false; reason: 'pin-predates-cycle' | 'landed-but-unmerged'; detail: string };

export function verifyPinIntegrity(run: WerkRun, werkDir: string): PinIntegrityVerdict {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- werkDir is CHORUS_WERK_BASE + validated role/card, no untrusted input
  if (!existsSync(werkDir)) return { ok: true };
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- same werkDir, constant '.git' suffix
    const cycleStart = statSync(path.join(werkDir, '.git')).birthtimeMs;
    const started = Date.parse(run.startedAt);
    // 2s slack: pin and worktree are stamped by the same clock, but never refuse
    // on a same-moment race — only on a pin genuinely older than the worktree.
    if (Number.isFinite(cycleStart) && !Number.isNaN(started) && started + 2000 < cycleStart) {
      return {
        ok: false,
        reason: 'pin-predates-cycle',
        detail: `pin startedAt=${run.startedAt} precedes this cycle's worktree (created ${new Date(cycleStart).toISOString()})`,
      };
    }
  } catch {
    /* no stat → cannot place the cycle boundary → no refusal on guesswork */
  }
  if (run.phase === 'landed') {
    try {
      const ahead = execFileSync('git', ['-C', werkDir, 'rev-list', '--count', 'origin/main..HEAD'], {
        encoding: 'utf8',
      }).trim();
      const n = parseInt(ahead, 10);
      if (Number.isFinite(n) && n > 0) {
        return {
          ok: false,
          reason: 'landed-but-unmerged',
          detail: `pin claims landed but the werk is ${n} commit(s) ahead of origin/main — nothing merged`,
        };
      }
    } catch {
      /* git hiccup → cannot verify the merge → no refusal on guesswork */
    }
  }
  return { ok: true };
}

/** The per-card log a detached act run streams to; its tail carries the WERK_EXIT
 *  sentinel the poll-time reconcile reads (durable, survives an mcp restart).
 *  #3664: legacy fallback only — new runs write runLogPath (per-RUN, never shared). */
export function logPath(card: number, dir: string = RUNS_DIR): string {
  assertCardId(card);
  return path.join(dir, `${card}.log`);
}

/** #3664 — THIS run's own log (runId-keyed). The shared per-card log meant every
 *  start truncated the previous run's output, so a relaunch destroyed the failed
 *  run's evidence (#3660: cause unrecoverable). One file per run; the record's
 *  `logFile` points at it; a retry can never clobber prior evidence. */
export function runLogPath(card: number, runId: string, dir: string = RUNS_DIR): string {
  assertCardId(card);
  const safe = runId.replace(/[^A-Za-z0-9._-]/g, '_');
  return path.join(dir, `${card}-${safe}.log`);
}

/** #3458 — poll-time transition: a detached act run writes its result to the log
 *  (WERK_EXIT=<code>), not back through the (returned-already) MCP call. On a
 *  re-invoke, advance a 'running' record to its real terminal phase by reading
 *  that log. null log/no-sentinel → still running (leave as-is); 0 → presented;
 *  non-zero → failed with the child reason. Returns the (possibly advanced) run. */
/** #3678 AC1 — what a running→presented transition stamps: presentedAt + the
 *  re-read patchId (the round owns its self-commits). Degrades to the recorded
 *  patchId on any source failure. */
function presentedExtras(patchIdSource?: () => string): Partial<WerkRun> {
  const extras: Partial<WerkRun> = { presentedAt: new Date().toISOString() };
  try {
    const p = patchIdSource?.();
    if (p) extras.patchId = p;
  } catch { /* degrade: recorded patchId stands */ }
  return extras;
}

export function reconcileRunning(
  card: number,
  dir: string = RUNS_DIR,
  // #3678 AC1 — at the running→presented transition the round RE-STAMPS its
  // patchId from this source (the werk's head at present time), so the
  // pipeline's own commits (doc-coherence churn etc.) are absorbed into the
  // round instead of superseding it — the #3592 poll-relaunch loop's root.
  // Absent source (legacy callers/tests) → recorded patchId stands.
  patchIdSource?: () => string,
): WerkRun | null {
  const run = readRun(card, dir);
  if (!run || run.phase !== 'running') return run;
  let log: string;
  try {
    // #3664 — read THIS run's own log; legacy records (no logFile) use the per-card path.
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- run.logFile was written by us (runLogPath: RUNS_DIR + sanitized runId); legacy path is RUNS_DIR + `${card}.log`, card asserted positive-int
    log = readFileSync(run.logFile ?? logPath(card, dir), 'utf8');
  } catch {
    return run; // no log yet → still running
  }
  const code = parseExitSentinel(log);
  if (code === null) return run; // act still in flight
  // #3664 — a go-run can exit 0 while the witness HELD it (go given, demo not proven:
  // werk.yml gates merge/deploy/accept on `proven`, so they were SKIPPED and the job
  // still succeeded). Marking that 'landed' is a lie — nothing merged. Surface it as
  // 'failed' with the held reason so the poll tells the truth and a re-invoke (after
  // recording the missing gate/gather/go) legitimately retries. Detection is the
  // STRUCTURED `WERK_HELD=<reason>` sentinel the workflow's outcome step writes
  // (Silas gather: free-text [HELD] grep was fragile coupling to GHA log format).
  const held = code === 0 && run.go ? parseHeldSentinel(log) : null;
  if (held) {
    return markPhase(card, 'failed', { failureReason: held.slice(0, FAILURE_REASON_MAX) }, dir);
  }
  // exit 0 → terminal success: a land run (go:true) reached 'landed'; a present
  // run (go:false) reached 'presented'. Non-zero → failed with the child reason.
  if (code === 0) {
    const extras = run.go ? {} : presentedExtras(patchIdSource);
    return markPhase(card, run.go ? 'landed' : 'presented', extras, dir);
  }
  return markPhase(card, 'failed', { failureReason: extractFailureReason(log, '', 'unknown') }, dir);
}

/** A 'running' record should finish in minutes; anything older is a dead run whose
 *  terminal-phase write was lost. 30 min is well past the slowest cold build+land. */
export const RUN_TTL_MS = 30 * 60 * 1000;

/** Is a process alive? `kill(pid, 0)` sends no signal but probes existence:
 *  ESRCH → gone (dead); EPERM → exists but owned by another user (alive). */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** #3458 (+ Wren #2) — is a run STALE: a 'running' record whose pid is dead, or
 *  past the TTL? Only 'running' can be stale (terminal phases are final). The
 *  impure liveness probe lives here; decideRunAction stays pure and takes the
 *  boolean. Belt+suspenders for the rare case where act's own durable terminal
 *  write was lost (e.g. an mcp-server churn mid-act). */
export function isRunStale(run: WerkRun, nowMs: number = Date.now(), ttlMs: number = RUN_TTL_MS): boolean {
  if (run.phase !== 'running') return false;
  if (typeof run.pid === 'number' && !pidAlive(run.pid)) return true;
  const started = Date.parse(run.startedAt);
  if (!Number.isNaN(started) && nowMs - started > ttlMs) return true;
  return false;
}
