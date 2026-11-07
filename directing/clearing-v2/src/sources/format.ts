// @domain: messages

import { DoorRole } from './models';

// #4885 — every Boston form here comes from the one renderer
import { bostonDay, bostonFormat, bostonHHMM } from '../../../../platform/chorus-sdk/time/boston';

export function formatBostonTime(isoOrDateString?: string): string {
  if (!isoOrDateString) return 'now';
  const d = new Date(isoOrDateString);
  if (isNaN(d.getTime())) return 'now';
  return bostonHHMM(d);
}

/** #4566 — a time in Boston: HH:MM when it is the same Boston day as now, else "Thu 13:11". */
export function formatBostonWhen(iso: string | undefined, now: Date = new Date()): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return '';
  const hhmm = bostonHHMM(d);
  if (bostonDay(d) === bostonDay(now)) return hhmm;
  return `${bostonFormat(d, { weekday: 'short' })} ${hhmm}`;
}

export function formatTodayHeader(date = new Date()): string {
  const weekday = bostonFormat(date, { weekday: 'long' });
  const month = bostonFormat(date, { month: 'short' });
  const day = bostonFormat(date, { day: 'numeric' });
  return `${weekday}, ${month} ${day} · ${bostonHHMM(date)}`;
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
