/**
 * #4592 — today's count per message kind: how many were sent, how many reached
 * Buzz, and the gap. Jeff, 2026-10-17 17:42: "im really not clear what % of
 * messages are in buzz and what are going old path very fuzzy to me".
 *
 * Sent comes from pulse's message store (every nudge and every Jeff input it
 * carried today); this page presents, it derives nothing from logs (see
 * pulse-presents.test.ts). Reached comes from the relay: today's #team notes.
 * A message counts as reached only when a note carries its exact text (as v1's
 * tailer publishes it), so a kind that skips Buzz, or a message the word cap
 * refused, shows as a gap instead of a matching total.
 *
 * Role replies have no sent count here: pulse does not carry them (they go
 * from each role's hooks straight to the relay), so the page says so rather
 * than guessing.
 */
import type { NostrEvent } from '../types';

export type MessageKind = 'jeff-input' | 'role-reply' | 'nudge-to-jeff' | 'nudge-between-roles' | 'system-alert';

export const KIND_LABELS: Record<MessageKind, string> = {
  'jeff-input': 'Jeff input',
  'role-reply': 'Role reply',
  'nudge-to-jeff': 'Nudge to Jeff',
  'nudge-between-roles': 'Nudge between roles',
  'system-alert': 'System alert',
};

/** One row of pulse's /api/messages. */
export interface PulseRow {
  type: string;
  from: string;
  to: string;
  content: string;
}

/** A sent message: its kind and the text it carries in the room. */
export interface Sent {
  kind: MessageKind;
  text: string;
}

/** Pulse stores a nudge with its delivery header; the room carries the words. */
function body(content: string): string {
  return content.replace(/^\[nudge from [^\]|]+\|[^\]]*\]\s*/, '');
}

/** One nudge row → what the room carries for it (clearing/src/tailer.ts). */
function nudgeSent(r: PulseRow): Sent | null {
  const text = body(r.content);
  if (!text) return null;
  if (r.to === 'jeff') return { kind: 'nudge-to-jeff', text };
  if (r.from === 'system') return { kind: 'system-alert', text: `[alert → ${r.to}] ${text}` };
  return { kind: 'nudge-between-roles', text: `[nudge from ${r.from} → ${r.to}] ${text}` };
}

/** Each pulse row → what was sent. One Jeff message is one row per role it is for: counted once. */
export function sentFromPulse(rows: PulseRow[]): Sent[] {
  const out: Sent[] = [];
  const jeff = new Set<string>();
  for (const r of rows) {
    if (r.type === 'nudge') {
      const s = nudgeSent(r);
      if (s) out.push(s);
    } else if (r.type === 'jeff-input' && !jeff.has(r.content)) {
      jeff.add(r.content);
      out.push({ kind: 'jeff-input', text: r.content });
    }
  }
  return out;
}

export interface KindCount {
  kind: MessageKind;
  label: string;
  /** null = not measured (role replies: see the top of this file). */
  sent: number | null;
  reached: number;
  gap: number | null;
}

/** Sent vs reached, per kind, each sent message matched to a note by its text. */
export function countKinds(sent: Sent[], notes: Pick<NostrEvent, 'content' | 'tags'>[]): KindCount[] {
  const texts = new Map<string, number>();
  let replies = 0;
  for (const n of notes) {
    if (n.tags.some((t) => t[0] === 't' && t[1] === 'reply')) replies++;
    else texts.set(n.content, (texts.get(n.content) || 0) + 1);
  }
  return (Object.entries(KIND_LABELS) as Array<[MessageKind, string]>).map(([kind, label]) => {
    if (kind === 'role-reply') return { kind, label, sent: null, reached: replies, gap: null };
    const mine = sent.filter((s) => s.kind === kind);
    let reached = 0;
    for (const s of mine) {
      const left = texts.get(s.text) || 0;
      if (left > 0) { reached++; texts.set(s.text, left - 1); }
    }
    return { kind, label, sent: mine.length, reached, gap: mine.length - reached };
  });
}

/** Start of today in Boston, as epoch seconds (the relay's `since`). */
export function bostonMidnight(now: Date = new Date()): number {
  const parts = new Intl.DateTimeFormat('en-US', { timeZone: 'America/New_York', hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
    .formatToParts(now).reduce<Record<string, string>>((a, p) => { a[p.type] = p.value; return a; }, {});
  const sinceMidnight = (Number(parts.hour) % 24) * 3600 + Number(parts.minute) * 60 + Number(parts.second);
  return Math.floor(now.getTime() / 1000) - sinceMidnight;
}

/** Today's nudges and Jeff inputs from pulse (created_at is UTC "YYYY-MM-DD HH:MM:SS"). */
export async function fetchPulseToday(pulseUrl: string, since: number, fetchFn: typeof fetch = fetch): Promise<PulseRow[]> {
  const sinceUtc = new Date(since * 1000).toISOString().replace('T', ' ').slice(0, 19);
  const rows: PulseRow[] = [];
  for (const type of ['nudge', 'jeff-input']) {
    const res = await fetchFn(`${pulseUrl}/api/messages?type=${type}&since=${encodeURIComponent(sinceUtc)}&limit=10000`);
    if (!res.ok) throw new Error(`pulse answered ${res.status} for ${type}`);
    rows.push(...(await res.json() as PulseRow[]));
  }
  return rows;
}
