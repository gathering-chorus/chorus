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
import path from 'path';
import os from 'os';
import type { WerkRun, WerkRunPhase } from './werk-run-state';

export const RUNS_DIR = path.join(os.homedir(), '.chorus', 'werk-runs');

function runPath(dir: string, card: number): string {
  return path.join(dir, `${card}.json`);
}

/** Read the run record for a card, or null (missing/malformed → null, never throws). */
export function readRun(card: number, dir: string = RUNS_DIR): WerkRun | null {
  try {
    const raw = readFileSync(runPath(dir, card), 'utf8');
    const obj = JSON.parse(raw) as WerkRun;
    if (obj && typeof obj.card === 'number' && typeof obj.phase === 'string') return obj;
    return null;
  } catch {
    return null;
  }
}

/** Write/overwrite the run record. Best-effort (a write failure must not break the verb). */
export function writeRun(run: WerkRun, dir: string = RUNS_DIR): void {
  try {
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
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
