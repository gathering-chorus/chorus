// @domain: messages
/**
 * #4634 — messages and events by day, as the herald sensors measured them.
 * Clearing v2 presents these readings and derives nothing: the counting rule
 * lives in pulse (message-counts.ts) and the sensors write SensorReadings.
 * When there is no reading, the page says unmeasured and why, never 0 (Wren).
 */

export interface Reading {
  readingOfSensor?: string;
  readingDay?: string;
  readingMetric?: string;
  readingValue?: string | number;
  readingRole?: string;
  /** #4647 — what a reading is about when one sensor measures several things (the sync sensor's crawled class). */
  readingSubject?: string;
}

/** The columns of the by-day table, in order: [sensor, metric, label]. */
export const DAY_COLUMNS: Array<[string, string, string]> = [
  ['messages', 'messages-sent', 'In messages.db'],
  ['messages', 'messages-kept', 'In Buzz'],
  ['messages', 'nudges', 'Nudges'],
  ['messages', 'jeff-inputs', 'Your messages'],
  ['messages', 'alerts', 'Alerts'],
  ['events', 'events', 'Events'],
  ['events', 'commands', 'Commands'],
  ['events', 'unreadable-lines', 'Unreadable lines'],
];

/** #4628 — doubles in Buzz per kind, the sensor's doubles-<kind> readings: their own short table, so it reads on a phone. */
export const DOUBLE_COLUMNS: Array<[string, string, string]> = [
  ['messages', 'doubles-jeff', 'Yours'],
  ['messages', 'doubles-reply', 'Replies'],
  ['messages', 'doubles-nudge', 'Nudges'],
  ['messages', 'doubles-alert', 'Alerts'],
  ['messages', 'doubles-land', 'Lands'],
];

/** #4647 — one crawled class on one day: files in git with no row, and rows out of date. null = unmeasured. */
export interface SyncClass { subject: string; missing: number | null; stale: number | null }
export interface SyncDay { day: string; classes: SyncClass[] }

/**
 * #4647 — the sync sensor's readings by day, newest first; within a day the
 * worst class first (Wren: reach, don't look). v2 derives nothing: the sensor
 * wrote missing and stale per class, this only arranges them.
 */
export function syncByDay(readings: Reading[]): SyncDay[] {
  const days = new Map<string, Map<string, SyncClass>>();
  for (const r of readings) {
    if (String(r.readingOfSensor || '').replace(/^sensor-/, '') !== 'sync' || !r.readingDay || !r.readingSubject) continue;
    if (r.readingMetric !== 'missing' && r.readingMetric !== 'stale') continue;
    const day = String(r.readingDay).slice(0, 10);
    const classes = days.get(day) || new Map<string, SyncClass>();
    const c = classes.get(r.readingSubject) || { subject: r.readingSubject, missing: null, stale: null };
    const v = Number(r.readingValue);
    c[r.readingMetric] = Number.isNaN(v) ? null : v;
    classes.set(r.readingSubject, c);
    days.set(day, classes);
  }
  const weight = (c: SyncClass) => (c.missing === null || c.stale === null ? Infinity : c.missing + c.stale);
  return [...days.keys()].sort().reverse().map((day) => ({
    day,
    classes: [...days.get(day)!.values()].sort((a, b) => weight(b) - weight(a) || a.subject.localeCompare(b.subject)),
  }));
}

export interface DayCell { metric: string; value: number | null }
export interface DayRow { day: string; values: DayCell[] }

/** Team totals by day, newest first; a column with no reading is null (shown as unmeasured). */
export function byDay(readings: Reading[], columns: Array<[string, string, string]> = DAY_COLUMNS): DayRow[] {
  const days = new Map<string, Map<string, number>>();
  for (const r of readings) {
    if (r.readingRole || !r.readingDay || !r.readingMetric || !r.readingOfSensor) continue;
    const day = String(r.readingDay).slice(0, 10);
    const m = days.get(day) || new Map<string, number>();
    // the door names the edge's target with its kind prefix (`sensor-messages`)
    m.set(`${String(r.readingOfSensor).replace(/^sensor-/, '')}/${r.readingMetric}`, Number(r.readingValue));
    days.set(day, m);
  }
  // a day with none of the table's metrics (today: only backup readings so far) is not a row
  return [...days.keys()].sort().reverse().filter((day) => columns.some(([s, m]) => days.get(day)!.has(`${s}/${m}`))).map((day) => ({
    day,
    values: columns.map(([s, m]) => {
      const v = days.get(day)!.get(`${s}/${m}`);
      return { metric: m, value: v === undefined || Number.isNaN(v) ? null : v };
    }),
  }));
}

/** Every reading the sensors wrote, from the door, all pages. Throws with the reason when it can't. */
export async function fetchReadings(readingsUrl: string, fetchFn: typeof fetch = fetch): Promise<Reading[]> {
  const out: Reading[] = [];
  let next: string | null = `${readingsUrl}?limit=1000`;
  for (let page = 0; next && page < 50; page++) {
    const res = await fetchFn(next, { signal: AbortSignal.timeout(10000) });
    if (!res.ok) throw new Error(`the readings door answered ${res.status}`);
    const body = await res.json() as { data?: Reading[]; links?: { next?: string } };
    out.push(...(body.data || []));
    next = body.links?.next ? new URL(body.links.next, readingsUrl).toString() : null;
  }
  return out;
}
