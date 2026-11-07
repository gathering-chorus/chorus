// Time utilities (extracted from server.ts for #2205 wave 6).
// Boston timestamps are the team's display default (#1826).

import { bostonLocal, bostonOffsetIso as sdkOffsetIso, isBostonDaylight } from '../../chorus-sdk/time/boston';

/**
 * #4885 — true when Boston keeps daylight time at this instant, from the tz database (the shared
 * renderer). It replaces a month rule that called the small hours of each change day wrong.
 */
export function isEDT(dateStr: string): boolean {
  const d = new Date(dateStr);
  return Number.isNaN(d.getTime()) ? false : isBostonDaylight(d);
}

/** Format an ISO timestamp into Boston-local `YYYY-MM-DD HH:MM:SS` (the shared renderer). */
export function convertToLocal(isoTimestamp: string, _tz: string): string {
  const d = new Date(isoTimestamp);
  return Number.isNaN(d.getTime()) ? isoTimestamp : bostonLocal(d);
}

/** Boston-local now, formatted via convertToLocal. */
export function bostonNow(): string {
  return convertToLocal(new Date().toISOString(), 'America/New_York');
}

/**
 * #4065 — Boston-local now as OFFSET ISO (`2026-09-02T11:08:15.123-0400`), the
 * spine's one clock (#3880 / ADR-058). `bostonNow()` is a naive display label
 * and must never be written to the spine: every /api/chorus/spine-event line
 * (seed.received, test.nohop, …) carried it and failed spine-one-clock nightly.
 */
export function bostonOffsetIso(d: Date = new Date()): string {
  // #4885 — the shared renderer; offset as -0400/-0500 (this API's spine contract since #4065)
  return sdkOffsetIso(d).replace(/([+-]\d{2}):(\d{2})$/, '$1$2');
}

/**
 * #3093 — single canonical render-to-Boston helper for human-facing strings.
 *
 * Storage stays UTC (correct for cross-machine ordering, spine events, db
 * rows). Render-to-human goes through this. The rule: anywhere a timestamp
 * lands in a string Jeff reads (alert message bodies, nudge bodies, terminal
 * output, bridge posts), use `boston()`. Anywhere it lands in storage (spine
 * event `ts` field, log JSON `timestamp` field, db column, JSON API contract),
 * leave as ISO. The render-vs-storage boundary is the discipline; this helper
 * is the only allowed render-time formatter.
 *
 * Format: `YYYY-MM-DD HH:MM:SS EDT` (or EST). Stable, sortable within a
 * timezone window, includes the suffix so the reader doesn't have to ask.
 * Accepts Date | string | number (epoch ms) — every common shape a Date can
 * arrive in, so callers don't have to pre-coerce.
 */
export function boston(ts: Date | string | number): string {
  const iso = ts instanceof Date
    ? ts.toISOString()
    : typeof ts === 'number'
      ? new Date(ts).toISOString()
      : ts;
  const local = convertToLocal(iso, 'America/New_York');
  // convertToLocal returns 'YYYY-MM-DD HH:MM:SS' (no TZ); add the abbreviation
  // so the reader knows the offset without doing the math.
  const tz = isEDT(iso) ? 'EDT' : 'EST';
  return `${local} ${tz}`;
}
