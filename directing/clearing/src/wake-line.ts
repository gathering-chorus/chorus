/**
 * #4339 — what pulse typed into a role's pane for a nudge until #4362: this
 * one fixed line. It still counts as a delivery for a transcript written then.
 *
 * #4362 — pulse now types the nudge itself ("[nudge from <who> | …] words").
 * A prompt is a delivery when it is the old line, or when pulse's messages.db
 * holds a delivered, headed nudge with exactly that text — the rule
 * chorus-hooks' is_relay and chorus-principal's is_delivery use. Every other
 * prompt is Jeff, including a "[nudge from" label he typed himself.
 */
import { execFileSync } from 'child_process';
import * as os from 'os';
import * as path from 'path';

export const WAKE_LINE = '[chorus] a message is waiting in your context under Pending nudges';

export function messagesDb(): string {
  if (process.env.CHORUS_MESSAGES_DB) return process.env.CHORUS_MESSAGES_DB;
  const root = process.env.CHORUS_ROOT_REPO || path.join(os.homedir(), 'CascadeProjects', 'chorus');
  return path.join(root, 'platform', 'pulse', 'messages.db');
}

/** Read-only lookup through the sqlite3 CLI; the text goes in as hex, so no
 * quoting can break the query. Any failure answers false (the words stay Jeff's). */
export function deliveredByPulse(text: string, db: string = messagesDb()): boolean {
  const t = text.trim();
  // a missing store makes sqlite3 exit non-zero, which the catch reads as false
  if (!t.startsWith('[nudge from ')) return false;
  const hex = Buffer.from(t, 'utf8').toString('hex').toUpperCase();
  const q = `SELECT 1 FROM messages WHERE type = 'nudge' AND delivery_status = 'delivered' AND trim(content) = CAST(X'${hex}' AS TEXT) LIMIT 1;`;
  try {
    return execFileSync('sqlite3', ['-readonly', db, q], { encoding: 'utf8', timeout: 2000 }).trim() === '1';
  } catch {
    return false;
  }
}

/** Was this prompt a delivery (not Jeff)? */
export function isDelivery(text: string, db?: string): boolean {
  return text.trim() === WAKE_LINE || deliveredByPulse(text, db);
}
