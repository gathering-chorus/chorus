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
import { readFileSync, writeFileSync, mkdirSync, existsSync, rmSync } from 'fs';
import { execFileSync } from 'child_process';
import path from 'path';
import os from 'os';
import { parseExitSentinel, extractFailureReason, type WerkRun, type WerkRunPhase } from './werk-run-state';

export const RUNS_DIR = path.join(os.homedir(), '.chorus', 'werk-runs');

/** #3538 — the werk's current patch-id: `git patch-id --stable` of
 *  merge-base(origin/main,HEAD)..HEAD, mirroring werk-demo's git_patch_id. Computed
 *  with two execFileSync calls piped via stdin (no shell → no injection surface).
 *  Best-effort: '' on ANY failure → the caller treats unknown as headChanged=false
 *  (attach), so a git hiccup degrades to today's behavior, never a spurious re-run. */
export function currentWerkPatchId(werkDir: string): string {
  try {
    const base = execFileSync('git', ['-C', werkDir, 'merge-base', 'origin/main', 'HEAD'], {
      encoding: 'utf8',
    }).trim();
    if (!base) return '';
    const diff = execFileSync('git', ['-C', werkDir, 'diff', `${base}..HEAD`], {
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    });
    if (!diff) return ''; // no diff vs main → no patch to key on
    const out = execFileSync('git', ['patch-id', '--stable'], { input: diff, encoding: 'utf8' });
    return out.trim().split(/\s+/)[0] || '';
  } catch {
    return '';
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

/** The per-card log a detached act run streams to; its tail carries the WERK_EXIT
 *  sentinel the poll-time reconcile reads (durable, survives an mcp restart). */
export function logPath(card: number, dir: string = RUNS_DIR): string {
  assertCardId(card);
  return path.join(dir, `${card}.log`);
}

/** #3458 — poll-time transition: a detached act run writes its result to the log
 *  (WERK_EXIT=<code>), not back through the (returned-already) MCP call. On a
 *  re-invoke, advance a 'running' record to its real terminal phase by reading
 *  that log. null log/no-sentinel → still running (leave as-is); 0 → presented;
 *  non-zero → failed with the child reason. Returns the (possibly advanced) run. */
export function reconcileRunning(card: number, dir: string = RUNS_DIR): WerkRun | null {
  const run = readRun(card, dir);
  if (!run || run.phase !== 'running') return run;
  let log = '';
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is RUNS_DIR + `${card}.log`; card asserted positive-int (assertCardId)
    log = readFileSync(logPath(card, dir), 'utf8');
  } catch {
    return run; // no log yet → still running
  }
  const code = parseExitSentinel(log);
  if (code === null) return run; // act still in flight
  // exit 0 → terminal success: a land run (go:true) reached 'landed'; a present
  // run (go:false) reached 'presented'. Non-zero → failed with the child reason.
  if (code === 0) return markPhase(card, run.go ? 'landed' : 'presented', {}, dir);
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
