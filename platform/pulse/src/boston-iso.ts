/**
 * boston-iso.ts — #3880: one clock on the spine.
 *
 * Every spine writer stamps offset-ISO America/New_York (the chorus-events
 * convention). This is THE formatter for TS writers; a second local variant
 * is the drift this card exists to end — import this, don't reimplement.
 * DST is computed from the tz database, never hardcoded (#1826's bug).
 */

export function bostonOffsetIso(d: Date = new Date()): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false, timeZoneName: 'longOffset',
  }).formatToParts(d);
  const get = (t: string) => parts.find(p => p.type === t)?.value ?? '';
  const hour = get('hour') === '24' ? '00' : get('hour');
  const raw = get('timeZoneName'); // e.g. "GMT-04:00"
  const offset = raw.replace('GMT', '') || '+00:00';
  const ms = String(d.getMilliseconds()).padStart(3, '0');
  return `${get('year')}-${get('month')}-${get('day')}T${hour}:${get('minute')}:${get('second')}.${ms}${offset}`;
}
