// @domain: messages

import { DoorRole } from './models';

const BOSTON_TZ = 'America/New_York';

export function formatBostonTime(isoOrDateString?: string): string {
  if (!isoOrDateString) return 'now';
  const d = new Date(isoOrDateString);
  if (isNaN(d.getTime())) return 'now';
  return d.toLocaleTimeString('en-US', {
    timeZone: BOSTON_TZ,
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    hourCycle: 'h23',
  });
}

/** #4566 — a time in Boston: HH:MM when it is the same Boston day as now, else "Thu 13:11". */
export function formatBostonWhen(iso: string | undefined, now: Date = new Date()): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const day = (x: Date) => x.toLocaleDateString('en-US', { timeZone: BOSTON_TZ });
  const hhmm = formatBostonTime(iso);
  if (day(d) === day(now)) return hhmm;
  const wd = d.toLocaleDateString('en-US', { timeZone: BOSTON_TZ, weekday: 'short' });
  return `${wd} ${hhmm}`;
}

export function formatTodayHeader(date = new Date()): string {
  const weekday = date.toLocaleDateString('en-US', { timeZone: BOSTON_TZ, weekday: 'long' });
  const month = date.toLocaleDateString('en-US', { timeZone: BOSTON_TZ, month: 'short' });
  const day = date.toLocaleDateString('en-US', { timeZone: BOSTON_TZ, day: 'numeric' });
  const time = date.toLocaleTimeString('en-US', { timeZone: BOSTON_TZ, hour: '2-digit', minute: '2-digit', hour12: false, hourCycle: 'h23' });
  return `${weekday}, ${month} ${day} · ${time}`;
}

/** Agent and human rows only; probe rows excluded. */
export function isPrincipalRow(r: DoorRole): boolean {
  if (!r.name) return false;
  if (r.name.startsWith('zz-probe') || r.name.startsWith('probe')) return false;
  const kind = (r.roleKind || '').toLowerCase();
  return kind === 'agent' || kind === 'human';
}

/** Display name from a door role's label or slug, without hardcoded literals. */
export function roleDisplayName(r: DoorRole): string {
  let name = r.label || r.name;
  if (name.startsWith('role-')) {
    name = name.replace('role-', '');
  }
  if (name.includes('(')) {
    name = name.split('(')[0].trim();
  }
  if (name.length > 0 && name[0] === name[0].toLowerCase()) {
    name = name.split('-').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
  }
  return name;
}

/** Last `:`-segment of an owner IRI/id with any principal-/role- prefix stripped. */
export function ownerBase(raw: string): string {
  return raw.split(':').pop()?.replace(/^(principal|role)-/, '') || raw;
}

export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
