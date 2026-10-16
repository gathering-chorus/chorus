// @domain: messages
// The three pulse reads Clearing v2 presents (#4563). Pulse derives the facts (actor state,
// cards changed overnight, source readings); v2 only fetches the envelope and shows it.
// Jeff 2026-10-16 08:13: "we use the data from our apis and donnt manufacture or do data processing in our ui".

import {
  ActorRow,
  ActorStateEnvelope,
  ChangesOvernightEnvelope,
  PulseActor,
  PulseChangedCard,
  PulseReading,
  PulseUnmeasured,
  ReadingsEnvelope,
  TodayCardItem,
  TrustStripSource,
} from './models';
import { fetchJson, failureReason } from './http';
import { formatBostonTime } from './format';

const PULSE_TIMEOUT_MS = 3000;

/** The envelope pulse returned, or the reason it could not be read. */
export type PulseRead<T> = { ok: true; value: T } | { ok: false; reason: string };

async function readPulse<T>(pulseUrl: string, route: string): Promise<PulseRead<T>> {
  const r = await fetchJson<T | null>(`${pulseUrl}${route}`, PULSE_TIMEOUT_MS);
  if (!r.ok) return { ok: false, reason: `pulse ${route} unreadable (${failureReason(r, 'connection refused')})` };
  if (!r.value) return { ok: false, reason: `pulse ${route} returned no body` };
  return { ok: true, value: r.value };
}

export function fetchActorState(pulseUrl: string): Promise<PulseRead<ActorStateEnvelope>> {
  return readPulse<ActorStateEnvelope>(pulseUrl, '/api/actor-state');
}

export function fetchChangesOvernight(pulseUrl: string): Promise<PulseRead<ChangesOvernightEnvelope>> {
  return readPulse<ChangesOvernightEnvelope>(pulseUrl, '/api/changes-overnight');
}

export function fetchReadings(pulseUrl: string): Promise<PulseRead<ReadingsEnvelope>> {
  return readPulse<ReadingsEnvelope>(pulseUrl, '/api/readings');
}

// ---- display formatting only: no state, count, diff or verdict is computed here ----

/** "principal-kade" -> "kade". */
export function shortPrincipal(id: string | undefined): string {
  return (id || '').replace(/^principal-/, '');
}

/** pulse's unmeasured entries as "source: reason; …", or undefined when there are none. */
export function unmeasuredText(entries: PulseUnmeasured[] | undefined): string | undefined {
  if (!entries || entries.length === 0) return undefined;
  return entries.map((u) => `${u.source}: ${u.reason}`).join('; ');
}

function sessionText(s: NonNullable<PulseActor['sessions']>[number]): string {
  const live = s.live === undefined ? '' : s.live ? ' · live' : ' · not live';
  const last = s.lastActionAt ? ` · ${formatBostonTime(s.lastActionAt)}` : '';
  return `${s.channel || s.id || 'session'}${live}${last}`;
}

function liveSessionLines(all: NonNullable<PulseActor['sessions']>): string[] {
  const live = all.filter((s) => s.live === true).map(sessionText);
  const rest = all.length - live.length;
  return rest > 0 ? [...live, `+${rest} not live`] : live;
}

export function toActorRow(a: PulseActor): ActorRow {
  const short = shortPrincipal(a.aboutPrincipal);
  const card = typeof a.currentCard === 'number'
    ? { id: a.currentCard, title: a.currentCardTitle || '' }
    : null;
  const w = a.waitingOn;
  return {
    id: short,
    name: a.name || short,
    state: a.stateValue,
    activity: a.activity,
    pair: a.pairRole && a.pairedWith ? `${a.pairRole} with ${shortPrincipal(a.pairedWith)}` : undefined,
    card,
    waiting: w ? [w.what, w.on ? `on ${shortPrincipal(w.on)}` : '', w.since ? `since ${w.since}` : ''].filter(Boolean).join(' ') : undefined,
    // display filter: live sessions listed, the rest counted (pulse decides live)
    sessions: liveSessionLines(a.sessions || []),
    unmeasured: unmeasuredText(a.unmeasuredSource),
  };
}

export function toChangedCard(c: PulseChangedCard): TodayCardItem {
  return {
    id: c.id,
    title: c.title || '',
    status: c.status || '',
    owner: shortPrincipal(c.owner),
    time: c.movedAt ? formatBostonTime(c.movedAt) : '',
  };
}

export function toTrustChip(r: PulseReading): TrustStripSource {
  if (r.status !== 'ok') return { source: r.source, status: 'unmeasured', reason: r.reason || r.status };
  const asOf = r.asOf || (r.lastWrite ? `as of ${formatBostonTime(r.lastWrite)}` : 'no as-of sent');
  return { source: r.source, status: 'ok', asOf, lagMinutes: r.lagMinutes };
}

export function unmeasuredChip(u: PulseUnmeasured): TrustStripSource {
  return { source: u.source, status: 'unmeasured', reason: u.reason };
}
