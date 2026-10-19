// @domain: messages
/**
 * #4634 — the messages sensor's Buzz reader. Prints one JSON line per Boston
 * day before today: {"day","sent","kept"} by the countKinds rule, read from
 * pulse's message store (read-only) and the relay's #team room (as the
 * bridge). The sensor (chorus-crawl messages) turns these into SensorReadings;
 * Clearing v2's /counts reads those readings and derives nothing.
 *
 *   node dist/buzz-day-counts.js [--db <messages.db>]
 * Env: PULSE_BUZZ_RELAY_URL, PULSE_BUZZ_RELAY_HOST (as the prod pulse plist).
 * Exit 3 = unmeasured (no relay, no key, no answer): print nothing, never 0.
 */
import os from 'os';
import path from 'path';
import Database from 'better-sqlite3';
import WebSocket from 'ws';
import { bridgeSignerFromHome, frameText, signNote } from './buzz-writer';
import { countDays, bostonDay, type DatedPulseRow, type RoomNote } from './message-counts';

export function readRows(db: string): DatedPulseRow[] {
  const d = new Database(db, { readonly: true, fileMustExist: true });
  try {
    return d.prepare(`SELECT created_at, type, "from", "to", content FROM messages WHERE type IN ('nudge','jeff-input')`).all() as DatedPulseRow[];
  } finally {
    d.close();
  }
}

function readNotes(url: string, host: string | undefined, since: number, timeoutMs = 60000): Promise<RoomNote[]> {
  const { signer, reason } = bridgeSignerFromHome(os.homedir());
  if (!signer) return Promise.reject(new Error(reason || 'no bridge key'));
  return new Promise((resolve, reject) => {
    const ws = host ? new WebSocket(url, { headers: { Host: host } }) : new WebSocket(url);
    const notes: RoomNote[] = [];
    const timer = setTimeout(() => { ws.close(); reject(new Error(`relay sent no EOSE within ${timeoutMs}ms`)); }, timeoutMs);
    const req = () => ws.send(JSON.stringify(['REQ', 'days', { kinds: [1], '#t': ['team', 'reply'], since, limit: 200000 }]));
    ws.on('open', () => setTimeout(req, 300));
    ws.on('error', (e) => { clearTimeout(timer); reject(e); });
    ws.on('message', (data) => {
      let f: unknown[];
      try { f = JSON.parse(frameText(data)) as unknown[]; } catch { return; }
      if (f[0] === 'AUTH' && typeof f[1] === 'string') {
        const relay = host ? `ws://${host}` : url;
        ws.send(JSON.stringify(['AUTH', signNote(signer, Math.floor(Date.now() / 1000), 22242, [['relay', relay], ['challenge', f[1]]], '')]));
        setTimeout(req, 300);
      } else if (f[0] === 'EVENT' && f[1] === 'days') {
        notes.push(f[2] as RoomNote);
      } else if (f[0] === 'EOSE' && f[1] === 'days') {
        clearTimeout(timer);
        ws.close();
        resolve(notes);
      }
    });
  });
}

export async function main(argv: string[] = process.argv, env: NodeJS.ProcessEnv = process.env): Promise<number> {
  const i = argv.indexOf('--db');
  const db = i > 0 ? argv[i + 1] : path.join(__dirname, '..', 'messages.db');
  const url = env.PULSE_BUZZ_RELAY_URL;
  if (!url) { process.stderr.write('buzz-day-counts: UNMEASURED — PULSE_BUZZ_RELAY_URL is not set\n'); return 3; }
  const rows = readRows(db);
  if (rows.length === 0) { process.stderr.write('buzz-day-counts: UNMEASURED — no messages in the store\n'); return 3; }
  const first = Math.min(...rows.map((r) => Date.parse(r.created_at.replace(' ', 'T') + 'Z')));
  let notes: RoomNote[];
  try {
    notes = await readNotes(url, env.PULSE_BUZZ_RELAY_HOST, Math.floor(first / 1000) - 86400);
  } catch (e) {
    process.stderr.write(`buzz-day-counts: UNMEASURED — ${(e as Error).message}\n`);
    return 3;
  }
  for (const d of countDays(rows, notes, bostonDay(Date.now()))) process.stdout.write(JSON.stringify(d) + '\n');
  return 0;
}

if (require.main === module) {
  main().then((c) => process.exit(c), (e) => { process.stderr.write(`buzz-day-counts: ${(e as Error).message}\n`); process.exit(3); });
}
