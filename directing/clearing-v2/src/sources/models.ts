// @domain: messages
// Page-facing data shapes (re-exported from data-service.ts) and the JSON shapes read from pulse and the doors.

export interface TrustStripSource {
  source: string;
  /** 'ok' / 'unmeasured' as pulse's readings say; 'failed' only for v2's own relay connection. */
  status: 'ok' | 'unmeasured' | 'failed';
  asOf?: string;
  lagMinutes?: number;
  reason?: string;
}

/** One actor as pulse's /api/actor-state reports it, formatted for display only. */
export interface ActorRow {
  id: string;
  name: string;
  /** pulse's stateValue, verbatim. */
  state: string;
  activity?: string;
  /** "driver with wren" when pulse names a pair role and partner. */
  pair?: string;
  card?: { id: number; title: string; step?: string } | null;
  cards?: Array<{ id: number; title: string; step?: string }>;
  stream?: Array<{ action: string; timestamp: string }>;
  /** "reply on kade since …" when pulse names what the actor waits on. */
  waiting?: string;
  sessions: string[];
  /** pulse's unmeasuredSource for this actor, as "source: reason". */
  unmeasured?: string;
}

export interface RolesData {
  actors: ActorRow[];
  /** Set when actor-state could not be read, or pulse listed unmeasured sources: shown by name, never as a zero. */
  unmeasured?: string;
}

export interface TodayCardItem {
  id: number;
  title: string;
  /** pulse's status, verbatim. */
  status: string;
  owner: string;
  time: string;
}

export interface TodayAlert {
  id: string;
  status: 'firing' | 'acked';
  title: string;
  resolvingCard: number;
  meta: string;
}

export interface TodayDecision {
  id: string;
  title: string;
  meeting: string;
  evidence: string[];
}

export interface TodayData {
  timeStr: string;
  decisions: TodayDecision[];
  decisionsCount: number;
  changedCards: TodayCardItem[];
  /** Set when changes-overnight could not be read or pulse listed unmeasured sources. */
  changedUnmeasured?: string;
  /** pulse's watermark, formatted; absent when pulse sent none. */
  watermark?: string;
  foldedCount: number;
  alerts: TodayAlert[];
  /** Set when the alerts API could not be read: named with reason, never 0 firing. */
  alertsUnmeasured?: string;
  workingRoles: ActorRow[];
  /** Set when actor-state could not be read or pulse listed unmeasured sources: named, never a zero. */
  workingUnmeasured?: string;
}

export interface RoomsData {
  room: string;
  channels: string[];
  directRoles: string[];
  relayConnected?: boolean;
  actors?: ActorRow[];
  messages: Array<{
    id: string;
    from: string;
    text: string;
    time: string;
    tick: boolean;
    cardId?: number;
    threadCount?: number;
    threadLast?: string;
    type?: string;
    kind?: 'nudge' | 'alert';
  }>;
}

// ---- JSON shapes read from sources ----

/** A row from the roles door (/v1/roles/roles). */
export interface DoorRole {
  name: string;
  label?: string;
  roleKind?: string;
}

export interface DoorRolesResponse {
  data?: DoorRole[];
}

export interface AlertItem {
  id?: string | number;
  name?: string;
  status?: string;
  state?: string;
  severity?: string;
  label?: string;
  title?: string;
  description?: string;
  trigger?: string;
  data?: string;
  meta?: string;
  resolvingCard?: string | number;
  links?: { resolvingCard?: string | number };
}

export interface AlertsResponse {
  data?: AlertItem[] | { alerts?: AlertItem[] };
}

export interface CardData {
  id?: number | string;
  label?: string;
  title?: string;
  status?: string;
  modified?: string;
  created?: string;
  assignee?: string | number;
  owner?: string | number;
  creator?: string | number;
  domain?: string;
}

export interface CardLinks {
  assignee?: string | number;
  ownedBy?: string | number;
}

/** A single card from the cards door (/v1/cards/cards/:id). */
export interface CardDoorResponse {
  kind?: string;
  data?: CardData | null;
  links?: CardLinks;
  owner?: string | number;
  servedFrom?: string;
}

// ---- pulse envelopes (#4563): pulse derives these facts; v2 presents them ----

export interface PulseUnmeasured {
  source: string;
  reason: string;
}

/** pulse's actor states; v2 shows the word it gets, known or not. */
export type ActorStateValue =
  | 'building' | 'pairing' | 'waiting' | 'in-ceremony' | 'reviewing' | 'available' | 'away' | 'off' | 'unmeasured';

export interface PulseActor {
  aboutPrincipal: string;
  name?: string;
  stateValue: ActorStateValue | string;
  stateSource?: string;
  activity?: 'acting' | 'idle' | 'unmeasured' | string;
  pairRole?: 'driver' | 'navigator' | string;
  pairedWith?: string;
  currentCard?: number | null;
  currentCardTitle?: string;
  cards?: Array<{ id: number; title: string; step?: string }>;
  stream?: Array<{ action: string; timestamp: string }>;
  waitingOn?: { on?: string; what?: string; since?: string };
  sessions?: Array<{ id?: string; channel?: string; live?: boolean; lastActionAt?: string }>;
  unmeasuredSource?: PulseUnmeasured[];
}

export interface ActorStateEnvelope {
  derivedAt?: string;
  actors?: PulseActor[];
  unmeasured?: PulseUnmeasured[];
}

export interface PulseChangedCard {
  id: number;
  title?: string;
  status?: string;
  owner?: string;
  movedAt?: string;
}

export interface ChangesOvernightEnvelope {
  derivedAt?: string;
  watermark?: string | null;
  cards?: PulseChangedCard[];
  unmeasured?: PulseUnmeasured[];
}

export interface PulseReading {
  source: string;
  observedAt?: string;
  status: 'ok' | 'unmeasured' | string;
  reason?: string;
  lastWrite?: string;
  asOf?: string;
  lagMinutes?: number;
}

export interface ReadingsEnvelope {
  derivedAt?: string;
  readings?: PulseReading[];
  unmeasured?: PulseUnmeasured[];
}
