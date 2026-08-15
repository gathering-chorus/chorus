/**
 * werk-phase.ts — #3883: a running pipeline is visible in Jeff's streams.
 *
 * Jeff (08-13): "no streams for 16 minutes, why do all of u stop"; (08-14):
 * "thats the last stream i see" — both mid-pipeline. The act run streams its
 * phases to a log file only roles read; the spine gets bookends. This module
 * follows the run log incrementally and emits `werk.phase` on every phase
 * transition (start / pass / fail), so the Streams pane shows "#3879 test
 * running 4m" instead of silence — and the WIP-drift watcher (#3879) counts
 * a running pipeline as card-attributed activity instead of drift.
 *
 * Scope ruling (Jeff, 08-14 17:39, on the card): streams must carry ALL
 * commands, not just shell — this is the pipeline leg of that.
 *
 * Pure parser + injectable follower (the #3458 DI-seam idiom). Timestamps:
 * offset-ISO Boston per #3880's one-clock contract — this writer is born
 * compliant.
 */

/** One phase transition parsed from act output. */
export interface PhaseEvent {
  phase: string;
  state: 'running' | 'pass' | 'fail';
  /** elapsed string act printed for finished phases, e.g. "16m43s" — raw */
  elapsed?: string;
}

const START = /^\[werk\/werk\]\s+⭐\s+Run Main (.+?)\s*$/;
const PASS = /^\[werk\/werk\]\s+✅\s+Success - Main (.+?) \[([^\]]+)\]/;
const FAIL = /^\[werk\/werk\]\s+❌\s+Failure - Main (.+?) \[([^\]]+)\]/;

/**
 * Parse NEW log content (any chunk of whole lines) into phase transitions.
 * Unknown lines are skipped — the parser must never throw on act noise.
 */
export function parsePhaseTransitions(chunk: string): PhaseEvent[] {
  const out: PhaseEvent[] = [];
  for (const line of chunk.split('\n')) {
    let m = line.match(START);
    if (m) { out.push({ phase: m[1], state: 'running' }); continue; }
    m = line.match(PASS);
    if (m) { out.push({ phase: m[1], state: 'pass', elapsed: m[2] }); continue; }
    m = line.match(FAIL);
    if (m) { out.push({ phase: m[1], state: 'fail', elapsed: m[2] }); }
  }
  return out;
}

export type ReadNew = () => Promise<string>;   // new bytes since last call
export type EmitPhase = (e: PhaseEvent) => Promise<void>;

/**
 * Follow one run: poll for new log bytes, emit each transition once, stop
 * when the log carries WERK_EXIT (the run's terminal marker) or on stop().
 * Emission failures are loud-not-fatal — visibility must never sink a run.
 */
export function followRun(
  readNew: ReadNew,
  emitPhase: EmitPhase,
  intervalMs = 15_000,
): () => void {
  let done = false;
  const timer = setInterval(() => {
    void (async () => {
      if (done) return;
      try {
        const chunk = await readNew();
        for (const ev of parsePhaseTransitions(chunk)) {
          await emitPhase(ev);
        }
        if (chunk.includes('WERK_EXIT=')) { done = true; clearInterval(timer); }
      } catch (e) {
        console.error('[werk-phase] follow failed:', e instanceof Error ? e.message : e);
      }
    })();
  }, intervalMs);
  return () => { done = true; clearInterval(timer); };
}

// ── live wiring ─────────────────────────────────────────────────────────────

import { open } from 'fs/promises';
import { appendFile } from 'fs/promises';
import * as os from 'os';
import * as path from 'path';

/** #3880 one-clock: offset-ISO Boston — THE formatter for this package's spine writers. */
export function bostonOffsetIso(d: Date = new Date()): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false, timeZoneName: 'longOffset',
  }).formatToParts(d);
  const get = (t: string) => parts.find(p => p.type === t)?.value ?? '';
  const hour = get('hour') === '24' ? '00' : get('hour');
  const offset = (get('timeZoneName') || 'GMT+00:00').replace('GMT', '');
  const ms = String(d.getMilliseconds()).padStart(3, '0');
  return `${get('year')}-${get('month')}-${get('day')}T${hour}:${get('minute')}:${get('second')}.${ms}${offset}`;
}

/**
 * Follow a live run's log file and stamp werk.phase onto the spine for every
 * transition — the pipeline becomes visible in Jeff's streams (#3883). Reads
 * incrementally from a byte offset; emission is append-only best-effort.
 */
export function wireRunFollower(
  logFile: string,
  meta: { card: number; role: string; runId: string },
  spinePath: string = process.env.CHORUS_LOG_FILE
    || path.join(os.homedir(), '.chorus', 'chorus.log'),
): () => void {
  let offset = 0;
  const readNew: ReadNew = async () => {
    try {
      const fh = await open(logFile, 'r');
      try {
        const { size } = await fh.stat();
        if (size <= offset) return '';
        const buf = Buffer.alloc(size - offset);
        await fh.read(buf, 0, buf.length, offset);
        offset = size;
        return buf.toString('utf8');
      } finally { await fh.close(); }
    } catch { return ''; } // log not created yet — next tick
  };
  const emitPhase: EmitPhase = async (e) => {
    const line = JSON.stringify({
      timestamp: bostonOffsetIso(), event: 'werk.phase', role: meta.role,
      card_id: meta.card, run_id: meta.runId,
      phase: e.phase, state: e.state, ...(e.elapsed ? { elapsed: e.elapsed } : {}),
    }) + '\n';
    try { await appendFile(spinePath, line); } catch { /* best-effort */ }
  };
  return followRun(readNew, emitPhase);
}
