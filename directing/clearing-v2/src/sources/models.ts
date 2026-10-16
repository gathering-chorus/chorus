// @domain: messages
// Page-facing data shapes (re-exported from data-service.ts) and the JSON shapes read from doors, board, Loki.

export interface TrustStripSource {
  source: string;
  lastWrite: string;
  asOf: string;
  status: 'ok' | 'failed';
  lagMinutes?: number;
  reason?: string;
}

export interface RolePrincipal {
  id: string;
  name: string;
  roleKind: 'human' | 'agent';
  loggedIn: string;
  state: 'working' | 'waiting on you' | 'blocked' | 'stale' | 'off' | 'idle' | 'unmeasured';
  card?: { id: number; title: string } | null;
  pane: string;
  lastHeard: string;
  waitingReason?: string;
}

export interface RolesData {
  principals: RolePrincipal[];
  error?: string;
}

export interface TodayCardItem {
  id: number;
  title: string;
  status: 'landed' | 'blocked' | 'moved' | 'wip';
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

export interface TodayWorkingRole {
  role: string;
  name: string;
  state: string;
  cardId: number;
  cardTitle: string;
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
  /** Set when the moves source could not be read: the panel names it instead of showing nothing. */
  changedUnmeasured?: string;
  foldedCount: number;
  alerts: TodayAlert[];
  workingRoles: TodayWorkingRole[];
}

export interface RoomsData {
  room: string;
  channels: string[];
  directRoles: string[];
  relayConnected?: boolean;
  messages: Array<{
    id: string;
    from: string;
    text: string;
    time: string;
    tick: boolean;
    cardId?: number;
    threadCount?: number;
    threadLast?: string;
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

export interface BoardWipCard {
  id: number;
  owner?: string;
  title: string;
}

/** Board context /wip response. */
export interface BoardWipResponse {
  timestamp?: string;
  data?: { cards?: BoardWipCard[] };
}

/** Loki instant-vector query response. */
export interface LokiQueryResponse {
  data?: {
    result?: Array<{
      metric?: { role?: string };
      value?: [number | string, string];
    }>;
  };
}

/** A row from the identity sessions door. */
export interface DoorSession {
  actsAs?: string;
  name?: string;
  ownedBy?: string;
  sessionState?: string;
  endedAt?: string;
}

export interface DoorSessionsResponse {
  data?: DoorSession[];
}

export interface AlertItem {
  id?: string | number;
  name?: string;
  status?: string;
  state?: string;
  label?: string;
  title?: string;
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

/** A row from the cards collection door. */
export interface CardRow extends CardData {
  name?: string;
  data?: CardData;
  links?: CardLinks;
}

export interface CardsResponse {
  data?: CardRow[];
}

/** A single card from the cards door (/v1/cards/cards/:id). */
export interface CardDoorResponse {
  kind?: string;
  data?: CardData | null;
  links?: CardLinks;
  owner?: string | number;
  servedFrom?: string;
}
