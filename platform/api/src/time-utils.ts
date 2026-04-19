// Time utilities (extracted from server.ts for #2205 wave 6).
// Boston timestamps are the team's display default (#1826).

/**
 * Approximate DST check: EDT runs from the second Sunday of March to the
 * first Sunday of November. Good enough for display labels — Intl handles
 * the actual conversion.
 */
export function isEDT(dateStr: string): boolean {
  const d = new Date(dateStr);
  const month = d.getMonth();
  if (month > 2 && month < 10) return true; // Apr–Oct always EDT
  if (month === 2) {
    // March: EDT starts second Sunday
    const firstDay = new Date(d.getFullYear(), 2, 1).getDay();
    const secondSunday = firstDay === 0 ? 8 : 15 - firstDay;
    return d.getDate() >= secondSunday;
  }
  if (month === 10) {
    // November: EDT ends first Sunday
    const firstDay = new Date(d.getFullYear(), 10, 1).getDay();
    const firstSunday = firstDay === 0 ? 1 : 8 - firstDay;
    return d.getDate() < firstSunday;
  }
  return false; // Dec–Feb always EST
}

/** Format an ISO timestamp into Boston-local `YYYY-MM-DD HH:MM:SS`. */
export function convertToLocal(isoTimestamp: string, _tz: string): string {
  try {
    const d = new Date(isoTimestamp);
    const fmt = new Intl.DateTimeFormat('en-US', {
      timeZone: 'America/New_York',
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit',
      hour12: false,
    });
    const parts = fmt.formatToParts(d);
    const get = (type: string) => parts.find(p => p.type === type)?.value || '';
    return `${get('year')}-${get('month')}-${get('day')} ${get('hour')}:${get('minute')}:${get('second')}`;
  } catch {
    return isoTimestamp;
  }
}

/** Boston-local now, formatted via convertToLocal. */
export function bostonNow(): string {
  return convertToLocal(new Date().toISOString(), 'America/New_York');
}
