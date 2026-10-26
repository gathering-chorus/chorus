/**
 * awareness-rows.ts — #4744 slice 3c: the rows pulse writes so the tree lives in the store.
 *
 * Pulse derives the tree's facts (pulse-tree.ts): Jeff's prompts, each role's last call,
 * and the turns and runs on the spine. Until now they lived only in pulse's memory, so
 * the generated pulse API (:3392) had nothing to serve: 0 Prompt, 0 Turn and 1 stale
 * RoleState rows (measured 2026-10-26 10:01). These are the same facts as rows of the
 * awareness classes, shaped to their SHACL fields, for pulse to write through pulse-api
 * as principal-pulse (Silas, 10:02). The tree Operation then reads only the store.
 *
 * Pure: no store, no network. Order matters on write: a Turn links its Prompt, so prompts
 * go first.
 */
import { joinPrompts, factsFromSpine, QUIET_AFTER_S, type LineFact, type SpineLine } from './pulse-tree';

export interface Row { [field: string]: string | string[] }
export interface AwarenessRows { prompts: Row[]; roleStates: Row[]; turns: Row[] }

/** A row name the door accepts: the id's own characters, lower case, anything else a dash. */
export const rowName = (id: string) => id.toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '');
const principal = (role: string) => `principal-${role}`;
const iso = (t: string) => new Date(t).toISOString().replace(/\.\d{3}Z$/, 'Z');
const short = (s: string) => (s.length > 80 ? s.slice(0, 79) + '…' : s);

export function awarenessRows(input: {
  now: string;
  actors: string[];
  lines: LineFact[];
  spine: SpineLine[];
  lastCalls: Record<string, string>;
}): AwarenessRows {
  const { prompts, lineToPrompt } = joinPrompts(input.lines, input.actors);
  const promptRows: Row[] = prompts.map(p => ({
    name: rowName(p.lines[0]),
    label: `prompt ${p.lines[0]} from ${p.by}`,
    comment: short(p.text.split('\n')[0]),
    promptText: p.text,
    promptedAt: iso(p.at),
    promptedBy: principal(p.by),
    promptTo: p.to.map(principal),
  }));
  const promptName = (line?: string) => {
    const id = line ? lineToPrompt.get(line) : undefined;
    return id ? `prompt-${rowName(id.replace(/^prompt-/, ''))}` : undefined;
  };

  const { turns } = factsFromSpine(input.spine, input.lastCalls);
  const actors = new Set(input.actors);
  const turnRows: Row[] = turns.filter(t => actors.has(t.role)).map(t => {
    const row: Row = {
      name: rowName(t.id),
      label: `turn of ${t.role} at ${iso(t.startedAt)}`,
      comment: t.wokenByLine ? `woken by line ${t.wokenByLine}` : 'woken by a nudge or a pipeline',
      turnOf: principal(t.role),
      turnStartedAt: iso(t.startedAt),
      lastCallAt: iso(t.lastCallAt),
    };
    if (t.endedAt) row.turnEndedAt = iso(t.endedAt);
    if (t.card !== undefined) row.onCard = `card-${t.card}`;
    const p = promptName(t.wokenByLine);
    if (p) row.answersPrompt = p;
    return row;
  });

  const lastCalls = new Map(Object.entries(input.lastCalls));
  const roleStates: Row[] = input.actors.flatMap(role => {
    const last = lastCalls.get(role);
    if (!last) return []; // never seen acting: no row, rather than a made-up time
    const quietS = Math.round((Date.parse(input.now) - Date.parse(last)) / 1000);
    return [{
      name: rowName(role),
      label: `${role} now`,
      comment: `last call ${quietS}s ago`,
      aboutPrincipal: principal(role),
      activity: quietS < QUIET_AFTER_S ? 'acting' : 'idle',
      // the declared state (building, waiting …) is not derived here: say so, never guess
      stateValue: 'unmeasured',
      stateSource: 'observed',
      unmeasuredSource: 'pulse derives moving/quiet only (#4744)',
      observedAt: iso(last),
    }];
  });
  return { prompts: promptRows, roleStates, turns: turnRows };
}
