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
import {
  readFileSync, writeFileSync, mkdirSync, existsSync, rmSync, renameSync, statSync,
  openSync, closeSync, fsyncSync, unlinkSync, linkSync,
} from 'fs';
import { execFileSync } from 'child_process';
import { randomUUID } from 'crypto';
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

/**
 * Atomically replace a run record, surfacing every failure to the caller.
 *
 * Launch paths use this fail-closed variant: write+fsync a unique sibling,
 * then rename it over the card record. A crash can leave an old complete
 * record or a complete new record, never truncated JSON that readRun() would
 * silently interpret as "no run" and turn into a duplicate launch.
 */
export function writeRunAtomic(run: WerkRun, dir: string = RUNS_DIR): void {
  assertCardId(run.card);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- dir is RUNS_DIR or a test-injected runs dir
  mkdirSync(dir, { recursive: true });
  const dest = runPath(dir, run.card);
  const temp = `${dest}.tmp-${process.pid}-${randomUUID()}`;
  let fd: number | undefined;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- unique sibling of validated card record
    fd = openSync(temp, 'wx', 0o600);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fd is our unique sibling opened immediately above
    writeFileSync(fd, JSON.stringify(run, null, 2), { encoding: 'utf8' });
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
    // POSIX rename within one directory is atomic and replaces the old record.
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- controlled sibling paths described above
    renameSync(temp, dest);
  } catch (error) {
    if (fd !== undefined) {
      try { closeSync(fd); } catch { /* preserve the original write error */ }
    }
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- cleanup of our unique temp only
      unlinkSync(temp);
    } catch { /* absent after a successful rename, or best-effort cleanup */ }
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`werk-run-store: atomic write failed for card ${run.card}: ${detail}`, { cause: error });
  }
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
    writeRunAtomic(run, dir);
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
/** #3956 — record one leg's verdict + the werk tree hash onto the run pin.
 *  Called ONLY from the run follower (one writer; the launcher never touches
 *  legs except to carry them at relaunch). Last write per leg wins. */
export function recordLeg(
  card: number,
  leg: string,
  verdict: 'pass' | 'fail',
  treeHash: string,
  dir: string = RUNS_DIR,
): void {
  const cur = readRun(card, dir);
  if (!cur) return; // pin gone (cleared/superseded) — nothing to record against
  // #3972 — a proof without a measured tree is no proof: an empty hash means
  // the measurement FAILED (index race, git error), and recording it would let
  // a later resume compare against "" and misreport 'tree changed'.
  if (!treeHash) return;
  const legs = (cur.legs ?? []).filter((l) => l.leg !== leg);
  legs.push({ leg, verdict, tree_hash: treeHash });
  writeRun({ ...cur, legs }, dir);
}

/** #3956 — the werk TREE hash including uncommitted edits: add -A into a
 *  throwaway index, write-tree. Mirrors platform/scripts/werk-resume-check
 *  tree_hash() — the two sides must agree byte-for-byte on the key.
 *  #3972 — the throwaway index is built with `read-tree HEAD`, NEVER by copying
 *  .git/index: a copy taken while a concurrent run writes the index is
 *  truncated, git aborts ("index file smaller than expected"), and the empty
 *  result poisoned resume as a phantom 'tree changed'. add -A restages the
 *  full working tree either way, so the hash is identical — minus the race.
 *  '' still means MEASUREMENT FAILED; callers treat it as unmeasured, never
 *  as a comparable value. */
export function currentWerkTreeHash(werkDir: string): string {
  try {
    const tmp = path.join(os.tmpdir(), `wt-index-${process.pid}-${Date.now()}`);
    try {
      execFileSync('git', ['-C', werkDir, 'read-tree', 'HEAD'], {
        env: { ...process.env, GIT_INDEX_FILE: tmp },
      });
      execFileSync('git', ['-C', werkDir, 'add', '-A'], {
        env: { ...process.env, GIT_INDEX_FILE: tmp },
      });
      return execFileSync('git', ['-C', werkDir, 'write-tree'], {
        env: { ...process.env, GIT_INDEX_FILE: tmp },
        encoding: 'utf8',
      }).trim();
    } finally {
      try { rmSync(tmp, { force: true }); } catch { /* best-effort */ }
    }
  } catch {
    return '';
  }
}

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

/** #3458 (+ Wren #2) — is a run STALE? PID truth outranks age:
 *  - a live PID is never stale, however old the run record is;
 *  - a dead PID is stale immediately;
 *  - a record with no usable PID gets the TTL backstop.
 * Only 'running' can be stale (terminal phases are final). The absent-PID case
 * is also the launch reservation window: persistence happens before spawn, so
 * a daemon crash there must suppress duplicate launch until the TTL expires. */
export function isRunStale(run: WerkRun, nowMs: number = Date.now(), ttlMs: number = RUN_TTL_MS): boolean {
  if (run.phase !== 'running') return false;
  if (typeof run.pid === 'number' && Number.isInteger(run.pid) && run.pid > 0) {
    return !pidAlive(run.pid);
  }
  const started = Date.parse(run.startedAt);
  if (!Number.isNaN(started) && nowMs - started > ttlMs) return true;
  return false;
}

export interface RunLaunchLockOptions {
  /** Maximum time to wait for another daemon's launch decision to finish. */
  timeoutMs?: number;
  /** Retry cadence while another daemon owns the card lock. */
  pollMs?: number;
}

const DEFAULT_LAUNCH_LOCK_TIMEOUT_MS = 30_000;
const DEFAULT_LAUNCH_LOCK_POLL_MS = 25;

type LaunchLockOwner = { token: string; pid: number; acquiredAt: string };

function launchLockPath(card: number, dir: string): string {
  assertCardId(card);
  return path.join(dir, `${card}.launch.lock`);
}

function sleepMs(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Only a complete lock whose recorded owner is positively dead is stale.
 * Malformed state is never stolen: fail-closed beats two launchers. */
function launchLockIsStale(lockFile: string): boolean {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- validated per-card lock path
    const owner = JSON.parse(readFileSync(lockFile, 'utf8')) as Partial<LaunchLockOwner>;
    if (typeof owner.pid === 'number' && Number.isInteger(owner.pid) && owner.pid > 0) {
      return !pidAlive(owner.pid);
    }
  } catch { /* malformed or disappearing lock: never infer ownership */ }
  return false;
}

/** Serialize stale-lock reclamation itself. Without this second exclusive file,
 * two waiters can both judge the old owner dead; one can unlink+reacquire while
 * the other then unlinks the NEW owner's lock. A crashed reaper intentionally
 * leaves this file behind and future launches time out fail-closed rather than
 * risk a duplicate act. */
function unlinkStaleLaunchLock(lockFile: string): boolean {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- validated per-card lock path
    unlinkSync(lockFile);
    return true;
  } catch (error) {
    // A concurrent release also clears the way for the next acquire.
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return true;
    throw error;
  }
}

function reclaimStaleLaunchLock(lockFile: string): boolean {
  const reaperFile = `${lockFile}.reaper`;
  let fd: number | undefined;
  let reclaimed = false;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- sibling of validated per-card lock path
    fd = openSync(reaperFile, 'wx', 0o600);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fd is our exclusive reaper file opened immediately above
    writeFileSync(fd, JSON.stringify({ pid: process.pid, acquiredAt: new Date().toISOString() }), { encoding: 'utf8' });
    fsyncSync(fd);
    if (launchLockIsStale(lockFile)) {
      // Any non-ENOENT unlink failure surfaces instead of creating a busy loop.
      reclaimed = unlinkStaleLaunchLock(lockFile);
    }
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST') return false;
    throw error;
  } finally {
    if (fd !== undefined) {
      try { closeSync(fd); } catch { /* release remains best-effort */ }
      try {
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- our exclusive reaper file
        unlinkSync(reaperFile);
      } catch { /* a crash/recovery race leaves launch fail-closed */ }
    }
  }
  return reclaimed;
}

function releaseRunLaunchLock(lockFile: string, token: string): () => void {
  return () => {
    try {
      // Token check prevents an old owner from unlinking a successor's lock.
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- validated per-card lock path
      const current = JSON.parse(readFileSync(lockFile, 'utf8')) as Partial<LaunchLockOwner>;
      // Token is an ownership identifier, not a secret; constant-time comparison is irrelevant.
      // eslint-disable-next-line security/detect-possible-timing-attacks
      if (current.token === token) {
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- validated per-card lock path
        unlinkSync(lockFile);
      }
    } catch { /* already reclaimed/removed: release is idempotent */ }
  };
}

/** Write a complete owner record at a unique, non-lock candidate path. */
function writeRunLaunchLockCandidate(candidate: string, owner: LaunchLockOwner, card: number): void {
  let fd: number | undefined;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- unique candidate includes UUID token
    fd = openSync(candidate, 'wx', 0o600);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- fd is our unique candidate opened immediately above
    writeFileSync(fd, JSON.stringify(owner), { encoding: 'utf8' });
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
  } catch (error) {
    if (fd !== undefined) {
      try { closeSync(fd); } catch { /* preserve the persistence error */ }
    }
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- cleanup of our unique candidate only
      unlinkSync(candidate);
    } catch { /* absent or best-effort candidate cleanup */ }
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`werk-run-store: launch lock candidate failed for card ${card}: ${detail}`, { cause: error });
  }
}

/** Publish a fully-written owner record with one no-clobber hard-link operation.
 * A paused writer is safe at every point:
 * - before link: only its unique candidate exists and does not block anyone;
 * - after link: the fixed lock path already exposes the complete PID+token.
 * There is no empty fixed-path window another process could reap by age. */
function tryCreateRunLaunchLock(lockFile: string, token: string, card: number): (() => void) | null {
  const candidate = `${lockFile}.candidate-${process.pid}-${token}`;
  const owner: LaunchLockOwner = { token, pid: process.pid, acquiredAt: new Date().toISOString() };
  writeRunLaunchLockCandidate(candidate, owner, card);
  try {
    // link(2) fails with EEXIST and never overwrites the current owner's path.
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- complete unique candidate to validated fixed lock path
    linkSync(candidate, lockFile);
    return releaseRunLaunchLock(lockFile, token);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST') return null;
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`werk-run-store: launch lock publish failed for card ${card}: ${detail}`, { cause: error });
  } finally {
    try {
      // The fixed hard link, when published, owns the inode independently.
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- cleanup of our unique candidate only
      unlinkSync(candidate);
    } catch { /* crash-safe orphan candidate is not an active lock */ }
  }
}

function reclaimLaunchLockIfStale(
  lockFile: string,
  card: number,
): boolean {
  if (!launchLockIsStale(lockFile)) return false;
  try {
    return reclaimStaleLaunchLock(lockFile);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`werk-run-store: stale launch-lock recovery failed for card ${card}: ${detail}`, { cause: error });
  }
}

export async function acquireRunLaunchLock(
  card: number,
  dir: string = RUNS_DIR,
  options: RunLaunchLockOptions = {},
): Promise<() => void> {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- dir is RUNS_DIR or a test-injected runs dir
  mkdirSync(dir, { recursive: true });
  const lockFile = launchLockPath(card, dir);
  const token = randomUUID();
  const timeoutMs = options.timeoutMs ?? DEFAULT_LAUNCH_LOCK_TIMEOUT_MS;
  const pollMs = options.pollMs ?? DEFAULT_LAUNCH_LOCK_POLL_MS;
  const deadline = Date.now() + timeoutMs;

  for (;;) {
    const release = tryCreateRunLaunchLock(lockFile, token, card);
    if (release) return release;
    if (reclaimLaunchLockIfStale(lockFile, card)) continue;
    if (Date.now() >= deadline) {
      throw new Error(`werk-run-store: timed out waiting ${timeoutMs}ms for card ${card} launch lock`);
    }
    await sleepMs(Math.max(1, pollMs));
  }
}

/**
 * Cross-process per-card launch critical section. Callers must put the entire
 * re-read → reconcile → decide → reserve → spawn → PID-record sequence inside.
 */
export async function withRunLaunchLock<T>(
  card: number,
  criticalSection: () => T | Promise<T>,
  dir: string = RUNS_DIR,
  options: RunLaunchLockOptions = {},
): Promise<T> {
  const release = await acquireRunLaunchLock(card, dir, options);
  try {
    return await criticalSection();
  } finally {
    release();
  }
}
