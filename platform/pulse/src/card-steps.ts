/**
 * card-steps.ts — #4757: each WIP card's pipeline step, as CardStep rows on pulse's header.
 *
 * Jeff, 2026-10-26: "i have been asking for state for like 9 months". The step is /cws's own
 * line (chorus-werk-status --json), never a second reading of dagu, so the header and /cws
 * cannot disagree. A card that leaves WIP is marked gone, so the header shows WIP cards only.
 *
 * Pure apart from `readCardSteps` (runs the script) and `readLiveSteps` (reads the stream).
 */
import { execFile } from 'child_process';
import * as path from 'path';
import { rowName, type Row } from './awareness-rows';

export interface CwsRow { card: string; role: string; step: string; live: boolean; since: string; run: string }

const principal = (role: string) => `principal-${role}`;

/** One CardStep row per WIP card, then a gone row for each live row whose card left WIP. */
export function cardStepRows(wip: CwsRow[], liveInStore: Row[]): Row[] {
  const rows: Row[] = wip.map(c => {
    const row: Row = {
      name: rowName(`step-${c.card}`),
      label: `#${c.card} step`,
      comment: `#${c.card} ${c.role} ${c.step}${c.run ? ` · ${c.run}` : ''}`,
      cardId: c.card,
      aboutPrincipal: principal(c.role),
      pipelineStep: c.step,
      stepLive: 'live',
    };
    if (c.since) row.stepSince = c.since;
    if (c.run) row.runOrdinal = c.run;
    return row;
  });
  const now = new Set(rows.map(r => r.name));
  for (const old of liveInStore) {
    if (now.has(String(old.name))) continue;
    const gone: Row = {};
    for (const f of ['name', 'label', 'comment', 'cardId', 'aboutPrincipal', 'pipelineStep', 'stepSince', 'runOrdinal']) {
      if (typeof old[f] === 'string' && old[f] !== '') gone[f] = old[f];
    }
    gone.stepLive = 'gone';
    rows.push(gone);
  }
  return rows;
}

/** chorus-werk-status --json. Rejects on a non-zero exit (an unreadable board is not an empty one). */
export function readCardSteps(root = process.env.CHORUS_HOME || path.resolve(__dirname, '../../..')): Promise<CwsRow[]> {
  return new Promise((resolve, reject) => {
    execFile('python3', [path.join(root, 'platform/scripts/chorus-werk-status'), '--json'], { timeout: 45_000 }, (err, out) => {
      if (err) return reject(new Error(`chorus-werk-status --json: ${err.message.split('\n')[0]}`));
      try { resolve(JSON.parse(String(out)) as CwsRow[]); } catch (e) { reject(e); }
    });
  });
}

/** The CardStep rows the store holds as live, link fields reduced to their names. */
export async function readLiveSteps(base = process.env.PULSE_API_URL || 'http://127.0.0.1:3392'): Promise<Row[]> {
  const res = await fetch(`${base}/pulse/awareness/cardsteps?stepLive=live&limit=100`, { signal: AbortSignal.timeout(4000) });
  if (!res.ok) throw new Error(`cardsteps stream: HTTP ${res.status}`);
  const body = await res.json() as { data?: Array<Record<string, unknown>> };
  const tail = (v: unknown) => (typeof v === 'string' ? v.replace(/^.*[#/]/, '') : '');
  return (body.data ?? []).map(r => ({
    name: tail(r.name), label: String(r.label ?? ''), comment: String(r.comment ?? ''), cardId: String(r.cardId ?? ''),
    aboutPrincipal: tail(r.aboutPrincipal), pipelineStep: String(r.pipelineStep ?? ''),
    stepSince: String(r.stepSince ?? ''), runOrdinal: String(r.runOrdinal ?? ''),
  }));
}
