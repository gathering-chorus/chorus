/**
 * card-steps.ts — #4757: each WIP card's pipeline step, read from /cws's own lines
 * (chorus-werk-status --json), never a second reading of dagu and never stored, so the
 * header and /cws cannot disagree.
 */
import { execFile } from 'child_process';
import * as path from 'path';

export interface CwsRow { card: string; role: string; step: string; live: boolean; since: string; run: string }

/**
 * chorus-werk-status --json. Rejects on a non-zero exit (an unreadable board is not an empty one).
 * #4807 — an aborted signal kills the child, so a timed-out read leaves no script behind.
 */
export function readCardSteps(root = process.env.CHORUS_HOME || path.resolve(__dirname, '../../..'), signal?: AbortSignal): Promise<CwsRow[]> {
  return new Promise((resolve, reject) => {
    execFile('python3', [path.join(root, 'platform/scripts/chorus-werk-status'), '--json'], { timeout: 45_000, signal }, (err, out) => {
      if (err) return reject(new Error(`chorus-werk-status --json: ${err.message.split('\n')[0]}`));
      try { resolve(JSON.parse(String(out)) as CwsRow[]); } catch (e) { reject(e); }
    });
  });
}
