/**
 * role-state-replay — #4028 AC2: replay the derivation over a past day of the
 * spine and print a table Jeff reads.
 *
 *   node dist/cli/role-state-replay.js 2026-08-28 [HH:MM ...] [--log ~/.chorus/chorus.log]
 *
 * For each sample time (Boston), the state of each role is computed by the
 * SAME function the live endpoint uses (stateFromStreams), over the events
 * that preceded that moment. The board's WIP at that moment is not recorded
 * anywhere, so the card column is reconstructed from the stream: the role's
 * last card.pulled in the prior 24 h with no card.accepted/unpulled after it.
 * That reconstruction is labelled as such in the header — it is not the
 * function's normal input.
 */

import * as fs from 'fs';
import * as readline from 'readline';
import { stateFromStreams, type SpineLine, type WipCardEntry } from '../derive-role-state';

const ROLES = ['silas', 'wren', 'kade'];
const LOOKBACK_MS = 60 * 60_000;      // what the live endpoint reads
const CARD_LOOKBACK_MS = 24 * 3600_000;
const CARD_EVENTS = new Set(['card.pulled', 'card.accepted', 'card.unpulled']);

export interface Args { day: string; samples: string[]; logPath: string }
export interface Row { sample: string; role: string; state: string; card: string; lastEvent: string; age: string }

/** Thrown instead of exiting so the argument rules are testable. main() below
 *  turns it back into the usage message + exit 2 an operator sees. */
export class UsageError extends Error {}

function usage(): never {
  throw new UsageError('usage: role-state-replay <YYYY-MM-DD> [HH:MM ...] [--log <path>]');
}

export function parseArgs(argv: string[]): Args {
  const logIdx = argv.indexOf('--log');
  const logPath = logIdx >= 0 ? (argv.slice(logIdx + 1, logIdx + 2).join('') || '') : `${process.env.HOME}/.chorus/chorus.log`;
  const rest = logIdx >= 0 ? argv.filter((_a, i) => i !== logIdx && i !== logIdx + 1) : argv;
  const day = rest.slice(0, 1).join('');
  if (!day || !/^\d{4}-\d{2}-\d{2}$/.test(day)) usage();
  const samples = rest.length > 1 ? rest.slice(1) : ['11:00', '11:25', '12:00', '15:33'];
  return { day, samples, logPath };
}

export function prevDay(day: string): string {
  const d = new Date(`${day}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() - 1);
  return d.toISOString().slice(0, 10);
}

export function toLine(p: Record<string, unknown>): SpineLine | null {
  if (typeof p.event !== 'string' || typeof p.timestamp !== 'string') return null;
  return {
    timestamp: p.timestamp,
    event: p.event,
    role: typeof p.role === 'string' ? p.role : undefined,
    card_id: typeof p.card_id === 'string' || typeof p.card_id === 'number' ? p.card_id : undefined,
    detail: typeof p.detail === 'string' ? p.detail : undefined,
    payload: typeof p.payload === 'string' ? p.payload : undefined,
  };
}

/** Stream the spine once; keep only the day's lines inside [earliest, latest]. */
export async function readDay(logPath: string, day: string, earliest: number, latest: number): Promise<SpineLine[]> {
  const kept: SpineLine[] = [];
  const dayTag = `"timestamp":"${day}`;
  const prevTag = `"timestamp":"${prevDay(day)}`;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- the operator names the spine to replay
  const rl = readline.createInterface({ input: fs.createReadStream(logPath, { encoding: 'utf8' }) });
  for await (const raw of rl) {
    if (!raw.includes(dayTag) && !raw.includes(prevTag)) continue;
    let parsed: Record<string, unknown>;
    try { parsed = JSON.parse(raw) as Record<string, unknown>; } catch { continue; }
    const ev = toLine(parsed);
    if (!ev) continue;
    const t = Date.parse(ev.timestamp);
    if (!Number.isFinite(t) || t < earliest || t > latest) continue;
    kept.push(ev);
  }
  return kept;
}

export function reconstructWip(role: string, events: SpineLine[], now: number): WipCardEntry[] {
  const open = new Set<string>();
  for (const e of events) {
    if (!CARD_EVENTS.has(e.event)) continue;
    const t = Date.parse(e.timestamp);
    if (t > now || now - t > CARD_LOOKBACK_MS) continue;
    if ((e.role ?? '').toLowerCase() !== role) continue;
    if (e.card_id === undefined) continue;
    const id = String(e.card_id);
    if (e.event === 'card.pulled') open.add(id); else open.delete(id);
  }
  return [...open].map((id) => ({ id: Number(id), owner: role })).filter((c) => Number.isFinite(c.id));
}

export function rowFor(day: string, hm: string, role: string, events: SpineLine[], window: SpineLine[], now: number): Row {
  const d = stateFromStreams({ role, events: window, wipCards: reconstructWip(role, events, now), now });
  let card = '—';
  if (d.card) card = `#${d.card}`;
  else if (d.multi_wip) card = 'multi';
  return {
    sample: `${day} ${hm}`,
    role,
    state: d.state,
    card,
    lastEvent: d.lastEvent ?? '—',
    age: d.lastActivity ? `${Math.round((now - Date.parse(d.lastActivity)) / 1000)}s ago` : '—',
  };
}

export function sampleRows(day: string, samples: string[], events: SpineLine[]): Row[] {
  const rows: Row[] = [];
  for (const hm of samples) {
    const now = Date.parse(`${day}T${hm}:00-04:00`); // Boston, DST
    const window = events.filter((e) => { const t = Date.parse(e.timestamp); return t <= now && now - t <= LOOKBACK_MS; });
    for (const role of ROLES) rows.push(rowFor(day, hm, role, events, window, now));
  }
  return rows;
}

const COLUMNS: Array<[string, (r: Row) => string]> = [
  ['sample (Boston)', (r) => r.sample],
  ['role', (r) => r.role],
  ['state', (r) => r.state],
  ['card (from card.pulled, reconstructed)', (r) => r.card],
  ['last event', (r) => r.lastEvent],
  ['last activity', (r) => r.age],
];

export function renderTable(rows: Row[]): string {
  const widths = COLUMNS.map(([title, get]) => rows.reduce((w, r) => Math.max(w, get(r).length), title.length));
  const line = (cells: string[]) => cells.map((v, i) => v.padEnd(widths.slice(i, i + 1).reduce((a, b) => a + b, 0) || v.length)).join('  ');
  const out = [line(COLUMNS.map(([title]) => title)), widths.map((w) => '-'.repeat(w)).join('  ')];
  for (const r of rows) out.push(line(COLUMNS.map(([, get]) => get(r))));
  return out.join('\n');
}

export async function main(): Promise<void> {
  const { day, samples, logPath } = parseArgs(process.argv.slice(2));
  const sampleMs = samples.map((hm) => Date.parse(`${day}T${hm}:00-04:00`));
  const events = await readDay(logPath, day, Math.min(...sampleMs) - CARD_LOOKBACK_MS, Math.max(...sampleMs));
  console.log(renderTable(sampleRows(day, samples, events)));
  console.log(`\n${events.length} spine lines read for ${day} (lookback ${LOOKBACK_MS / 60000} min per sample); same function as GET /api/chorus/context/roles.`);
}

// Only run when invoked as a command. Importing this module (the test does)
// must not read the spine or print a table.
if (require.main === module) {
  main().catch((e: unknown) => {
    if (e instanceof UsageError) { console.error(e.message); process.exit(2); }
    console.error(e);
    process.exit(1);
  });
}
