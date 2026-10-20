// @domain: messages
/**
 * #4634 — moved here from Clearing v2 (#4592), so the messages sensor derives
 * and v2 only presents: one rule, one copy (Kade, Wren 2026-10-18).
 *
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
/** The note fields the count reads. */
export interface RoomNote { content: string; tags: string[][]; created_at?: number; kind?: number }

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
export function countKinds(sent: Sent[], notes: RoomNote[]): KindCount[] {
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

/** The Boston calendar day of a UTC instant. */
export function bostonDay(ms: number): string {
  return new Intl.DateTimeFormat('en-CA', { timeZone: 'America/New_York', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(ms));
}

/** A pulse row with its UTC created_at ("YYYY-MM-DD HH:MM:SS"). */
export interface DatedPulseRow extends PulseRow { created_at: string }

/**
 * #4634 — per Boston day before `today`: how many messages should have reached
 * Buzz (sent, by the countKinds rule) and how many did (kept). Role replies
 * have no sent figure, so they are left out of both. Days with no message at
 * all are not invented here; the sensor fills them with a measured 0.
 */
export function countDays(rows: DatedPulseRow[], notes: RoomNote[], today: string): Array<{ day: string; sent: number; kept: number }> {
  const rowsByDay = new Map<string, PulseRow[]>();
  for (const r of rows) {
    const day = bostonDay(Date.parse(r.created_at.replace(' ', 'T') + 'Z'));
    if (day >= today) continue;
    rowsByDay.set(day, [...(rowsByDay.get(day) || []), r]);
  }
  const notesByDay = new Map<string, RoomNote[]>();
  for (const n of notes) {
    if (typeof n.created_at !== 'number') continue;
    const day = bostonDay(n.created_at * 1000);
    notesByDay.set(day, [...(notesByDay.get(day) || []), n]);
  }
  return [...rowsByDay.keys()].sort().map((day) => {
    const kinds = countKinds(sentFromPulse(rowsByDay.get(day) || []), notesByDay.get(day) || []).filter((k) => k.sent !== null);
    return { day, sent: kinds.reduce((a, k) => a + (k.sent || 0), 0), kept: kinds.reduce((a, k) => a + k.reached, 0) };
  });
}

/** #4628 — the kinds a doubles reading is kept for, one row per day each. */
export const DOUBLE_KINDS = ['jeff', 'reply', 'nudge', 'alert', 'land'] as const;
export type DoubleKind = (typeof DOUBLE_KINDS)[number];

const tag = (n: RoomNote, k: string) => n.tags.find((t) => t[0] === k)?.[1];

/** Which kind a room note is, from its tags (whichever writer posted it). */
export function noteKind(n: RoomNote): DoubleKind {
  const k = tag(n, 'kind');
  if (k === 'nudge' || k === 'alert' || k === 'jeff') return k;
  if (tag(n, 't') === 'reply' || tag(n, 'role')) return 'reply';
  const author = tag(n, 'author');
  if (author === 'jeff') return 'jeff';
  if (!author || author === 'unattributed') return 'land';
  return 'reply';
}

/** A role's reply header ("--- Silas | … ---"); its own key keeps it, v1's copy dropped it. */
const words = (content: string) => content.replace(/^--- [^\n]* ---\s*/, '').trim();

/**
 * #4628 AC3 — doubles per Boston day per kind, for days before `today`. A
 * double is the #4646 rule the room uses: same author, same words, within
 * 20 s of an earlier note. Every kind gets a row for a day with notes, so 0 is
 * a reading and not a silence.
 */
export function countDoubles(notes: RoomNote[], today: string, windowS = 20): Array<{ day: string; kind: DoubleKind; doubles: number }> {
  const dated = notes.filter((n) => typeof n.created_at === 'number').sort((a, b) => (a.created_at as number) - (b.created_at as number));
  const last = new Map<string, number>();
  const counts = new Map<string, Map<DoubleKind, number>>();
  for (const n of dated) {
    const t = n.created_at as number;
    const day = bostonDay(t * 1000);
    if (day >= today) continue;
    const row = counts.get(day) ?? new Map(DOUBLE_KINDS.map((k) => [k, 0]));
    counts.set(day, row);
    const key = `${tag(n, 'author') ?? tag(n, 'role') ?? ''}\u0000${words(n.content)}`;
    const prev = last.get(key);
    if (prev !== undefined && t - prev <= windowS) row.set(noteKind(n), (row.get(noteKind(n)) ?? 0) + 1);
    last.set(key, t);
  }
  return [...counts.keys()].sort().flatMap((day) => DOUBLE_KINDS.map((kind) => ({ day, kind, doubles: counts.get(day)?.get(kind) ?? 0 })));
}
