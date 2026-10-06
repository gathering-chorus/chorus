/**
 * #4420 — a werk run can be cancelled or paused (Jeff, 2026-10-02: "it doesnt
 * make sense that pipelines cant be cancelled or paused").
 *
 *  - cancel: stops act and every child (the run is spawned detached, so its pid
 *    leads its own process group), marks the run 'cancelled', and writes the
 *    terminal marker into the run's own log so a later poll reads the truth.
 *  - pause: a hold file the pipeline checks between steps (werk.yml waits while
 *    it exists); the step in flight finishes, the next one waits.
 *  - resume: removes the hold file; the next step starts.
 *
 * Side effects come in through `deps`, so every decision is testable without a
 * live act.
 */
import * as fs from 'fs';
import { execFileSync, spawn } from 'child_process';
import * as path from 'path';
import type { WerkRun } from './werk-run-state';

export const CANCEL_MARKER = 'WERK_EXIT=cancelled';

export interface ControlDeps {
  /** Signal a whole process group (negative pid). Throws ESRCH when it is gone. */
  killGroup: (pid: number, signal: NodeJS.Signals) => void;
  isAlive: (pid: number) => boolean;
  /** When the process holding `pid` started, or null if unknown. */
  startedAt: (pid: number) => Date | null;
  writeRun: (run: WerkRun) => void;
  appendLog: (file: string, line: string) => void;
  sleep: (ms: number) => Promise<void>;
  now: () => Date;
}

export type ControlResult =
  | { ok: true; phase: WerkRun['phase']; paused?: boolean; note: string }
  | { ok: false; refusal: 'no-run' | 'not-running'; note: string };

export function holdFilePath(card: number, runsDir: string): string {
  return path.join(runsDir, `${card}.hold`);
}

export function isHeld(card: number, runsDir: string): boolean {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- runsDir is RUNS_DIR or a test dir; card is a validated integer
  return fs.existsSync(holdFilePath(card, runsDir));
}

/** Cancel a live run: TERM the group, KILL whatever is left after `graceMs`. */
export async function cancelRun(run: WerkRun | null, deps: ControlDeps, runsDir: string, graceMs = 5000): Promise<ControlResult> {
  if (!run) return { ok: false, refusal: 'no-run', note: 'No run on record for this card — nothing to cancel.' };
  if (run.phase !== 'running') {
    return { ok: false, refusal: 'not-running', note: `The run on record is '${run.phase}', not running — nothing to cancel.` };
  }
  // A pid outlives its run: on 2026-10-06 a Sep 19 pin still named pid 30554,
  // and a TERM by pid alone hit whatever process holds that number now. Signal
  // only the process that started when this run did.
  if (typeof run.pid === 'number' && deps.isAlive(run.pid) && sameProcess(run, deps)) {
    try { deps.killGroup(run.pid, 'SIGTERM'); } catch { /* already gone */ }
    const step = 250;
    for (let waited = 0; waited < graceMs && deps.isAlive(run.pid); waited += step) await deps.sleep(step);
    if (deps.isAlive(run.pid)) {
      try { deps.killGroup(run.pid, 'SIGKILL'); } catch { /* already gone */ }
    }
  }
  try { fs.rmSync(holdFilePath(run.card, runsDir), { force: true }); } catch { /* none */ }
  if (run.logFile) deps.appendLog(run.logFile, `${CANCEL_MARKER}\n`);
  const cancelled: WerkRun = { ...run, phase: 'cancelled', failureReason: `cancelled at ${deps.now().toISOString()}` };
  deps.writeRun(cancelled);
  return { ok: true, phase: 'cancelled', note: `Run ${run.runId} cancelled: act and its children stopped, the run record and its log say cancelled.` };
}

/** The live pid is this run's own process: it started within a minute of the run. */
export function sameProcess(run: WerkRun, deps: Pick<ControlDeps, 'startedAt'>): boolean {
  if (typeof run.pid !== 'number') return false;
  const began = deps.startedAt(run.pid);
  const pinned = Date.parse(run.startedAt);
  if (!began || Number.isNaN(pinned)) return false;
  return Math.abs(began.getTime() - pinned) <= 60_000;
}

/** Hold a live run between steps. */
export function pauseRun(run: WerkRun | null, runsDir: string): ControlResult {
  if (!run) return { ok: false, refusal: 'no-run', note: 'No run on record for this card — nothing to pause.' };
  if (run.phase !== 'running') {
    return { ok: false, refusal: 'not-running', note: `The run on record is '${run.phase}', not running — nothing to pause.` };
  }
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- runsDir is RUNS_DIR or a test dir
  fs.mkdirSync(runsDir, { recursive: true });
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- the card's own hold file under runsDir
  fs.writeFileSync(holdFilePath(run.card, runsDir), `${run.runId}\n`);
  return { ok: true, phase: 'running', paused: true, note: `Run ${run.runId} pauses after the step in flight; the next step waits until resume.` };
}

/** Release a paused run. */
export function resumeRun(run: WerkRun | null, runsDir: string): ControlResult {
  if (!run) return { ok: false, refusal: 'no-run', note: 'No run on record for this card — nothing to resume.' };
  const held = isHeld(run.card, runsDir);
  fs.rmSync(holdFilePath(run.card, runsDir), { force: true });
  return {
    ok: true, phase: run.phase, paused: false,
    note: held ? `Run ${run.runId} resumed: the next step starts.` : `Run ${run.runId} was not paused.`,
  };
}

export function liveControlDeps(writeRun: (run: WerkRun) => void): ControlDeps {
  return {
    killGroup: (pid, signal) => process.kill(-pid, signal),
    isAlive: (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } },
    startedAt: (pid) => {
      try {
        const out = execFileSync('ps', ['-o', 'lstart=', '-p', String(pid)], { encoding: 'utf8' }).trim();
        const d = new Date(out);
        return out && !Number.isNaN(d.getTime()) ? d : null;
      } catch { return null; }
    },
    writeRun,
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- the run's own log path from its record
    appendLog: (file, line) => fs.appendFileSync(file, line),
    sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    now: () => new Date(),
  };
}

/** After a cancel: the slot's variant goes down with the run, and the spine says why. */
export function afterCancel(role: string, card: number, runId: string, binDir: string, scriptsDir: string): void {
  try {
    spawn(path.join(binDir, 'werk-deploy'), ['env-down', role, String(card)], { detached: true, stdio: 'ignore' }).unref();
    spawn('bash', [path.join(scriptsDir, 'chorus-log'), 'werk.cancelled', role, `card=${card}`, `run_id=${runId}`],
      { detached: true, stdio: 'ignore' }).unref();
  } catch { /* best-effort: the run record and log already say cancelled */ }
}
