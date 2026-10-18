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
}

/** The columns of the by-day table, in order: [sensor, metric, label]. */
export const DAY_COLUMNS: Array<[string, string, string]> = [
  ['messages', 'messages-sent', 'Sent'],
  ['messages', 'messages-kept', 'Kept in Buzz'],
  ['messages', 'nudges', 'Nudges'],
  ['messages', 'jeff-inputs', 'Your messages'],
  ['messages', 'alerts', 'Alerts'],
  ['events', 'events', 'Events'],
  ['events', 'commands', 'Commands'],
  ['events', 'unreadable-lines', 'Unreadable lines'],
];

export interface DayRow { day: string; values: Array<number | null> }

/** Team totals by day, newest first; a column with no reading is null (shown as unmeasured). */
export function byDay(readings: Reading[]): DayRow[] {
  const days = new Map<string, Map<string, number>>();
  for (const r of readings) {
    if (r.readingRole || !r.readingDay || !r.readingMetric || !r.readingOfSensor) continue;
    const day = String(r.readingDay).slice(0, 10);
    const m = days.get(day) || new Map<string, number>();
    m.set(`${r.readingOfSensor}/${r.readingMetric}`, Number(r.readingValue));
    days.set(day, m);
  }
  // a day with none of the table's metrics (today: only backup readings so far) is not a row
  return [...days.keys()].sort().reverse().filter((day) => DAY_COLUMNS.some(([s, m]) => days.get(day)!.has(`${s}/${m}`))).map((day) => ({
    day,
    values: DAY_COLUMNS.map(([s, m]) => {
      const v = days.get(day)!.get(`${s}/${m}`);
      return v === undefined || Number.isNaN(v) ? null : v;
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
