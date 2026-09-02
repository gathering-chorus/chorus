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

function usage(): never {
  console.error('usage: role-state-replay <YYYY-MM-DD> [HH:MM ...] [--log <path>]');
  process.exit(2);
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  const logIdx = argv.indexOf('--log');
  const logPath = logIdx >= 0 ? argv[logIdx + 1] : `${process.env.HOME}/.chorus/chorus.log`;
  const args = logIdx >= 0 ? argv.filter((_a, i) => i !== logIdx && i !== logIdx + 1) : argv;
  const day = args[0];
  if (!day || !/^\d{4}-\d{2}-\d{2}$/.test(day)) usage();
  const samples = args.slice(1).length ? args.slice(1) : ['11:00', '11:25', '12:00', '15:33'];
  // Boston offset for the day: -04:00 in DST; the spine stamps local time with offset.
  const sampleMs = samples.map((hm) => Date.parse(`${day}T${hm}:00-04:00`));
  const earliest = Math.min(...sampleMs) - CARD_LOOKBACK_MS;
  const latest = Math.max(...sampleMs);

  const kept: SpineLine[] = [];
  const cardEvents: SpineLine[] = [];
  const rl = readline.createInterface({ input: fs.createReadStream(logPath, { encoding: 'utf8' }) });
  for await (const line of rl) {
    // cheap gate: the day (or the day before, for the 24 h card lookback) must appear
    if (!line.includes(`"timestamp":"${day}`) && !line.includes(`"timestamp":"${prevDay(day)}`)) continue;
    let p: Record<string, unknown>;
    try { p = JSON.parse(line); } catch { continue; }
    if (typeof p.event !== 'string' || typeof p.timestamp !== 'string') continue;
    const t = Date.parse(p.timestamp);
    if (!Number.isFinite(t) || t < earliest || t > latest) continue;
    const ev: SpineLine = {
      timestamp: p.timestamp,
      event: p.event,
      role: typeof p.role === 'string' ? p.role : undefined,
      card_id: (p.card_id as string | number | undefined) ?? undefined,
      detail: typeof p.detail === 'string' ? p.detail : undefined,
      payload: typeof p.payload === 'string' ? p.payload : undefined,
    };
    if (ev.event === 'card.pulled' || ev.event === 'card.accepted' || ev.event === 'card.unpulled') cardEvents.push(ev);
    kept.push(ev);
  }

  const rows: string[][] = [];
  for (let i = 0; i < samples.length; i++) {
    const now = sampleMs[i];
    const window = kept.filter((e) => { const t = Date.parse(e.timestamp); return t <= now && now - t <= LOOKBACK_MS; });
    for (const role of ROLES) {
      const wip = reconstructWip(role, cardEvents, now);
      const d = stateFromStreams({ role, events: window, wipCards: wip, now });
      const age = d.lastActivity ? `${Math.round((now - Date.parse(d.lastActivity)) / 1000)}s ago` : '—';
      rows.push([`${day} ${samples[i]}`, role, d.state, d.card ? `#${d.card}` : (d.multi_wip ? 'multi' : '—'), d.lastEvent ?? '—', age]);
    }
  }
  const head = ['sample (Boston)', 'role', 'state', 'card (from card.pulled, reconstructed)', 'last event', 'last activity'];
  const widths = head.map((h, c) => Math.max(h.length, ...rows.map((r) => r[c].length)));
  const fmt = (r: string[]) => r.map((v, c) => v.padEnd(widths[c])).join('  ');
  console.log(fmt(head));
  console.log(widths.map((w) => '-'.repeat(w)).join('  '));
  for (const r of rows) console.log(fmt(r));
  console.log(`\n${kept.length} spine lines read for ${day} (lookback ${LOOKBACK_MS / 60000} min per sample); same function as GET /api/chorus/context/roles.`);
}

function prevDay(day: string): string {
  const d = new Date(`${day}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() - 1);
  return d.toISOString().slice(0, 10);
}

function reconstructWip(role: string, cardEvents: SpineLine[], now: number): WipCardEntry[] {
  const open = new Map<string, true>();
  for (const e of cardEvents) {
    const t = Date.parse(e.timestamp);
    if (t > now || now - t > CARD_LOOKBACK_MS) continue;
    if ((e.role ?? '').toLowerCase() !== role) continue;
    const id = e.card_id !== undefined ? String(e.card_id) : null;
    if (!id) continue;
    if (e.event === 'card.pulled') open.set(id, true);
    else open.delete(id);
  }
  return [...open.keys()].map((id) => ({ id: Number(id), owner: role })).filter((c) => Number.isFinite(c.id));
}

main().catch((e) => { console.error(e); process.exit(1); });
