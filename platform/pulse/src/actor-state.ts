/**
 * actor-state.ts — #4560: Actor state derived once in pulse.
 *
 * Implements the RoleState model from awareness-4018.ttl:
 * Precedence: in-ceremony > pairing > reviewing > waiting > building > away > off.
 * Exposes pure deriveActorState() plus ActorStateService with 15s cache.
 */

import { readdirSync, readFileSync, existsSync } from 'fs';
import * as path from 'path';
import * as os from 'os';

export type StateValue =
  | 'building'
  | 'pairing'
  | 'waiting'
  | 'in-ceremony'
  | 'reviewing'
  | 'available'
  | 'away'
  | 'off'
  | 'unmeasured';

export type ActivityValue = 'acting' | 'idle' | 'unmeasured';

export interface SessionInfo {
  id: string;
  principal: string;
  channel: 'pane' | 'browser' | string;
  expiresAt: string;
  lastSeenAt: string;
  endedAt?: string;
  sessionState?: string;
  live?: boolean;
}

export interface ActivePairFact {
  card: number;
  title?: string;
  navigator: string;
  driver?: string;
}

export interface OwedNudgeFact {
  from: string;
  to: string;
  expects: 'reply' | 'decision' | 'action';
  sentAt: string;
  repliedAt?: string;
  content?: string;
}

export interface WipCardFact {
  cardId: number;
  title?: string;
  owner: string;
  step?: string;
}

export interface ActionFact {
  lastActionAt?: string;
  recent5mCount?: number;
  recent30mCount?: number;
  recentActions?: Array<{ action: string; timestamp: string }>;
}

export interface PrincipalInfo {
  principal: string;
  name?: string;
  roleKind?: 'agent' | 'human' | string;
}

export interface ReviewCardFact {
  cardId: number;
  title?: string;
  reviewer: string;
  peer?: string;
}

export interface RoleCardFact {
  id: number;
  title: string;
  step?: string;
}

export interface ActorFactInput {
  now?: Date;
  principals: Array<string | PrincipalInfo>;
  activePairs?: ActivePairFact[];
  sessions?: SessionInfo[];
  actions?: Record<string, ActionFact>;
  owedNudges?: OwedNudgeFact[];
  wipCards?: WipCardFact[];
  reviewCards?: ReviewCardFact[];
  cardSteps?: Record<number, string>;
  unmeasured?: Array<{ source: string; reason: string }>;
}

export interface RoleStateResult {
  aboutPrincipal: string;
  name?: string;
  stateValue: StateValue;
  stateSource: 'observed';
  activity: ActivityValue;
  pairRole?: 'driver' | 'navigator';
  pairedWith?: string;
  currentCard?: number | null;
  currentCardTitle?: string;
  cards: RoleCardFact[];
  stream?: Array<{ action: string; timestamp: string }>;
  observedAt: string;
  waitingOn?: { on: string; what: string; since: string };
  sessions: Array<{ id: string; channel: string; live: boolean; lastActionAt?: string }>;
  unmeasuredSource?: Array<{ source: string; reason: string }>;
}

export interface ActorStateApiResponse {
  derivedAt: string;
  actors: RoleStateResult[];
  unmeasured: Array<{ source: string; reason: string }>;
}

const FIVE_MINUTES_MS = 5 * 60 * 1000;
const THIRTY_MINUTES_MS = 30 * 60 * 1000;

function principalToRole(principal: string): string {
  return principal.replace(/^principal-/, '');
}

function roleToPrincipal(role: string): string {
  return role.startsWith('principal-') ? role : `principal-${role}`;
}

export function isTeamRole(r: { name?: string; roleKind?: string }): boolean {
  const kind = r.roleKind?.toLowerCase();
  return kind === 'agent' || kind === 'human' || kind === 'person';
}

function deriveActivity(
  actionFact: ActionFact | undefined,
  nowMs: number,
  lokiUnmeasured: boolean,
  hasSessions: boolean,
  hasUnmeasured: boolean
): ActivityValue {
  if (lokiUnmeasured) return 'unmeasured';
  if (!actionFact) return hasSessions ? 'idle' : (hasUnmeasured ? 'unmeasured' : 'idle');

  const lastActionMs = actionFact.lastActionAt ? new Date(actionFact.lastActionAt).getTime() : 0;
  const count5m = actionFact.recent5mCount ?? 0;

  if (count5m > 0 || (lastActionMs > 0 && nowMs - lastActionMs <= FIVE_MINUTES_MS)) {
    return 'acting';
  }
  return 'idle';
}

interface DerivedSessionsResult {
  sessionResults: Array<{ id: string; channel: string; live: boolean; lastActionAt?: string }>;
  hasLiveSession: boolean;
}

function deriveSessions(
  mySessions: SessionInfo[],
  nowMs: number,
  lastActionMs: number,
  lastActionIso?: string
): DerivedSessionsResult {
  const sessionResults: Array<{ id: string; channel: string; live: boolean; lastActionAt?: string }> = [];
  let hasLiveSession = false;

  for (const s of mySessions) {
    const expiresMs = new Date(s.expiresAt).getTime();
    const lastSeenMs = new Date(s.lastSeenAt).getTime();
    const isExpired = isNaN(expiresMs) || nowMs > expiresMs;
    const isClosed = Boolean(s.endedAt) || s.sessionState === 'closed';

    const lastActivityMs = Math.max(isNaN(lastSeenMs) ? 0 : lastSeenMs, lastActionMs);
    const isStale = lastActivityMs === 0 || nowMs - lastActivityMs > THIRTY_MINUTES_MS;

    const live = !isClosed && !isExpired && !isStale;
    if (live) hasLiveSession = true;

    let sessionLastAction: string = s.lastSeenAt;
    if (lastActionMs === lastActivityMs && lastActionIso) {
      sessionLastAction = lastActionIso;
    } else if (lastActivityMs > 0) {
      sessionLastAction = new Date(lastActivityMs).toISOString();
    }

    sessionResults.push({
      id: s.id,
      channel: s.channel,
      live,
      lastActionAt: sessionLastAction,
    });
  }

  return { sessionResults, hasLiveSession };
}

interface DerivedStateOutcome {
  stateValue: StateValue;
  pairRole?: 'driver' | 'navigator';
  pairedWith?: string;
  currentCard?: number | null;
  waitingOn?: { on: string; what: string; since: string };
  unmeasuredSource?: Array<{ source: string; reason: string }>;
}

function evaluatePairing(
  role: string,
  activePairs: ActivePairFact[],
  wipCards: WipCardFact[]
): DerivedStateOutcome | undefined {
  const match = activePairs.find((p) => {
    const isWip = wipCards.some((w) => w.cardId === p.card);
    if (!isWip) return false;
    const nav = principalToRole(p.navigator);
    const drv = p.driver ? principalToRole(p.driver) : undefined;
    return nav === role || drv === role;
  });
  if (!match) return undefined;

  const isNav = principalToRole(match.navigator) === role;
  const rawPartner = isNav ? match.driver : match.navigator;
  const hasPartner = Boolean(rawPartner && rawPartner !== 'unknown' && rawPartner !== 'principal-unknown');

  return {
    stateValue: 'pairing',
    pairRole: isNav ? 'navigator' : 'driver',
    pairedWith: hasPartner && rawPartner ? roleToPrincipal(rawPartner) : undefined,
    currentCard: match.card,
    unmeasuredSource: hasPartner
      ? undefined
      : [{ source: 'pair-registry', reason: `no partner recorded on #${match.card}` }],
  };
}

function evaluateReviewing(role: string, reviewCards: ReviewCardFact[]): DerivedStateOutcome | undefined {
  const match = reviewCards.find((r) => principalToRole(r.reviewer) === role);
  if (!match) return undefined;
  return {
    stateValue: 'reviewing',
    currentCard: match.cardId,
  };
}

function parseTimestampMs(raw?: string): number {
  if (!raw) return 0;
  const normalized = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(raw)
    ? raw.replace(' ', 'T') + 'Z'
    : raw;
  const ms = Date.parse(normalized);
  return Number.isNaN(ms) ? 0 : ms;
}

function isNudgeCleared(nudge: OwedNudgeFact, allOwed: OwedNudgeFact[]): boolean {
  const fromRole = principalToRole(nudge.from);
  const toRole = principalToRole(nudge.to);
  const sentMs = parseTimestampMs(nudge.sentAt);
  const repliedMs = parseTimestampMs(nudge.repliedAt);
  if (repliedMs > 0 && repliedMs >= sentMs) return true;

  const reverseNudge = allOwed.find(
    (other) => principalToRole(other.from) === toRole && principalToRole(other.to) === fromRole
  );
  if (!reverseNudge) return false;

  const reverseMs = parseTimestampMs(reverseNudge.sentAt);
  return reverseMs > 0 && sentMs > 0 && reverseMs >= sentMs;
}

/** #4575 — messages.db stamps created_at with SQLite's UTC "YYYY-MM-DD HH:MM:SS" (no zone).
 *  Served as-is, every reader took it for Boston time: Jeff's phone showed "waiting since
 *  Fri 12:07" at 06:33. Serve it as an ISO instant; anything already zoned passes through. */
export function utcIso(sqliteTs: string): string {
  return /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(sqliteTs) ? `${sqliteTs.replace(' ', 'T')}Z` : sqliteTs;
}

function evaluateWaiting(
  role: string,
  hasLiveSession: boolean,
  owedNudges: OwedNudgeFact[]
): DerivedStateOutcome | undefined {
  if (!hasLiveSession) return undefined;
  const match = owedNudges.find(
    (n) => principalToRole(n.from) === role && !isNudgeCleared(n, owedNudges)
  );
  if (!match) return undefined;
  return {
    stateValue: 'waiting',
    waitingOn: {
      on: roleToPrincipal(match.to),
      what: match.expects,
      since: utcIso(match.sentAt),
    },
  };
}

function evaluateBuilding(
  role: string,
  wipCards: WipCardFact[],
  actingOrLive: boolean
): DerivedStateOutcome | undefined {
  if (!actingOrLive) return undefined;
  const match = wipCards.find((c) => principalToRole(c.owner) === role);
  if (!match) return undefined;
  return {
    stateValue: 'building',
    currentCard: match.cardId,
  };
}

interface StateEvaluationFacts {
  role: string;
  isHuman: boolean;
  activity: ActivityValue;
  hasLiveSession: boolean;
  hasRecentAction30m: boolean;
  activePairs: ActivePairFact[];
  reviewCards: ReviewCardFact[];
  owedNudges: OwedNudgeFact[];
  wipCards: WipCardFact[];
}

function evaluateState(facts: StateEvaluationFacts): DerivedStateOutcome {
  const pairing = evaluatePairing(facts.role, facts.activePairs, facts.wipCards);
  if (pairing) return pairing;

  const reviewing = evaluateReviewing(facts.role, facts.reviewCards);
  if (reviewing) return reviewing;

  const isActingOrLive = facts.isHuman
    ? facts.hasLiveSession || facts.activity === 'acting' || facts.hasRecentAction30m
    : facts.hasLiveSession;

  const waiting = evaluateWaiting(facts.role, facts.hasLiveSession, facts.owedNudges);
  const building = evaluateBuilding(facts.role, facts.wipCards, isActingOrLive);

  if (building && facts.activity === 'acting') {
    return { ...building, waitingOn: waiting?.waitingOn };
  }
  if (waiting) return waiting;
  if (building) return building;

  if (isActingOrLive) {
    return {
      stateValue: 'available',
    };
  }

  return {
    stateValue: facts.isHuman ? 'away' : 'off',
  };
}

function applyDoorFailures(
  stateOutcome: DerivedStateOutcome,
  unmeasured: Array<{ source: string; reason: string }> | undefined,
  hasLiveOrActing: boolean
): StateValue {
  if (!unmeasured || unmeasured.length === 0) {
    return stateOutcome.stateValue;
  }
  const rolesUnmeasured = unmeasured.some((u) => u.source.includes('role'));
  if (rolesUnmeasured) {
    return 'unmeasured';
  }
  const sessionsUnmeasured = unmeasured.some(
    (u) => u.source.includes('identity') || u.source.includes('session')
  );
  const isBaseState = stateOutcome.stateValue === 'away' || stateOutcome.stateValue === 'off';
  if (sessionsUnmeasured && isBaseState && !hasLiveOrActing) {
    return 'unmeasured';
  }
  return stateOutcome.stateValue;
}

function formatPrincipalName(role: string): string {
  if (role === 'abby-normal') return 'Abby Normal';
  return role.charAt(0).toUpperCase() + role.slice(1);
}

function findCardTitle(cardId: number | null | undefined, input: ActorFactInput): string | undefined {
  if (!cardId) return undefined;
  const fromWip = input.wipCards?.find((c) => c.cardId === cardId);
  if (fromWip?.title) return fromWip.title;
  const fromReview = input.reviewCards?.find((c) => c.cardId === cardId);
  if (fromReview?.title) return fromReview.title;
  const fromPair = input.activePairs?.find((p) => p.card === cardId);
  if (fromPair?.title) return fromPair.title;
  return undefined;
}

function resolvePrincipalInfo(p: string | PrincipalInfo): {
  principal: string;
  role: string;
  name: string;
  isHuman: boolean;
} {
  if (typeof p === 'string') {
    const role = principalToRole(p);
    return { principal: p, role, name: formatPrincipalName(role), isHuman: false };
  }
  const isHuman = p.roleKind === 'human' || p.roleKind === 'person';
  const role = principalToRole(p.principal);
  const name = p.name || formatPrincipalName(role);
  return { principal: p.principal, role, name, isHuman };
}

function extractActionFact(actionMap: Map<string, ActionFact>, role: string, principal: string): {
  actionFact: ActionFact | undefined;
  lastActionMs: number;
} {
  const fact = actionMap.get(role) ?? actionMap.get(principal);
  const lastActionMs = fact?.lastActionAt ? new Date(fact.lastActionAt).getTime() : 0;
  return { actionFact: fact, lastActionMs };
}

function combineUnmeasured(
  a?: Array<{ source: string; reason: string }>,
  b?: Array<{ source: string; reason: string }>
): Array<{ source: string; reason: string }> | undefined {
  const merged = [...(a ?? []), ...(b ?? [])];
  return merged.length > 0 ? merged : undefined;
}

function resolveMyWipCards(
  input: ActorFactInput,
  role: string,
  stateOutcome: DerivedStateOutcome
): RoleCardFact[] {
  const myWipCards: RoleCardFact[] = (input.wipCards ?? [])
    .filter((c) => principalToRole(c.owner) === role)
    .map((c) => ({
      id: c.cardId,
      title: c.title || '',
      step: c.step || input.cardSteps?.[c.cardId] || 'coding',
    }));

  if (stateOutcome.stateValue === 'pairing' && stateOutcome.currentCard) {
    if (!myWipCards.some((c) => c.id === stateOutcome.currentCard)) {
      const fromWip = input.wipCards?.find((c) => c.cardId === stateOutcome.currentCard);
      const pairedTitle = findCardTitle(stateOutcome.currentCard, input) || '';
      const pairedStep = fromWip?.step || input.cardSteps?.[stateOutcome.currentCard] || 'coding';
      myWipCards.unshift({
        id: stateOutcome.currentCard,
        title: pairedTitle,
        step: pairedStep,
      });
    }
  }

  return myWipCards;
}

function resolveActorStream(
  actionFact: ActionFact | undefined,
  finalStateValue: string
): Array<{ action: string; timestamp: string }> {
  if (actionFact?.recentActions && actionFact.recentActions.length > 0) {
    return actionFact.recentActions;
  }
  if (actionFact?.lastActionAt) {
    return [{ action: finalStateValue, timestamp: actionFact.lastActionAt }];
  }
  return [];
}

function deriveOneActor(
  p: string | PrincipalInfo,
  input: ActorFactInput,
  actionMap: Map<string, ActionFact>,
  nowMs: number,
  lokiUnmeasured: boolean,
  unmeasured: Array<{ source: string; reason: string }> | undefined,
  observedAt: string
): RoleStateResult {
  const { principal, role, name, isHuman } = resolvePrincipalInfo(p);
  const allSessions = input.sessions ?? [];
  const mySessions = allSessions.filter((s) => s.principal === principal);
  const { actionFact, lastActionMs } = extractActionFact(actionMap, role, principal);

  const activity = deriveActivity(
    actionFact,
    nowMs,
    lokiUnmeasured,
    mySessions.length > 0,
    Boolean(unmeasured)
  );

  const { sessionResults, hasLiveSession } = deriveSessions(
    mySessions,
    nowMs,
    lastActionMs,
    actionFact?.lastActionAt
  );

  const hasRecentAction30m = lastActionMs > 0 && nowMs - lastActionMs <= THIRTY_MINUTES_MS;
  const stateOutcome = evaluateState({
    role,
    isHuman,
    activity,
    hasLiveSession,
    hasRecentAction30m,
    activePairs: input.activePairs ?? [],
    reviewCards: input.reviewCards ?? [],
    owedNudges: input.owedNudges ?? [],
    wipCards: input.wipCards ?? [],
  });

  const finalStateValue = applyDoorFailures(
    stateOutcome,
    unmeasured,
    hasLiveSession || activity === 'acting'
  );

  const myWipCards = resolveMyWipCards(input, role, stateOutcome);
  const currentCard = stateOutcome.currentCard ?? (myWipCards.length > 0 ? myWipCards[0].id : null);
  const currentCardTitle = findCardTitle(currentCard, input);
  const stream = resolveActorStream(actionFact, finalStateValue);

  return {
    aboutPrincipal: principal,
    name,
    stateValue: finalStateValue,
    stateSource: 'observed',
    activity,
    pairRole: stateOutcome.pairRole,
    pairedWith: stateOutcome.pairedWith,
    currentCard,
    currentCardTitle,
    cards: myWipCards,
    stream,
    observedAt,
    waitingOn: stateOutcome.waitingOn,
    sessions: sessionResults,
    unmeasuredSource: combineUnmeasured(unmeasured, stateOutcome.unmeasuredSource),
  };
}

export function deriveActorState(input: ActorFactInput): RoleStateResult[] {
  const now = input.now ?? new Date();
  const nowMs = now.getTime();
  const observedAt = now.toISOString();
  const actionMap = new Map<string, ActionFact>(Object.entries(input.actions ?? {}));
  const unmeasured = input.unmeasured && input.unmeasured.length > 0 ? input.unmeasured : undefined;
  const lokiUnmeasured = Boolean(unmeasured?.some((u) => u.source.includes('loki')));

  return input.principals.map((p) =>
    deriveOneActor(p, input, actionMap, nowMs, lokiUnmeasured, unmeasured, observedAt)
  );
}

function parsePairJson(raw: string): { navigator: string; driver?: string } {
  try {
    const parsed = JSON.parse(raw) as { navigator?: string; driver?: string };
    return {
      navigator: parsed.navigator || '',
      driver: parsed.driver,
    };
  } catch (parseErr: unknown) {
    void parseErr;
    return { navigator: '' };
  }
}

/* eslint-disable security/detect-non-literal-fs-filename -- dynamic ops directory path */
function readPairDriverFromMd(pairMdPath: string): string | undefined {
  if (!existsSync(pairMdPath)) return undefined;
  try {
    const mdContent = readFileSync(pairMdPath, 'utf8');
    const m = mdContent.match(/Driver:\s*([a-z0-9-]+)/i);
    return m ? m[1].toLowerCase() : undefined;
  } catch (readErr: unknown) {
    void readErr;
    return undefined;
  }
}

function parsePairFile(dir: string, f: string): ActivePairFact | undefined {
  if (!f.endsWith('.json')) return undefined;
  const cardStr = f.replace('.json', '');
  const card = parseInt(cardStr, 10);
  if (isNaN(card)) return undefined;

  try {
    const raw = readFileSync(path.join(dir, f), 'utf8');
    const { navigator, driver: parsedDriver } = parsePairJson(raw);
    const driver = parsedDriver || readPairDriverFromMd(`/tmp/pair-${card}.md`);
    return { card, navigator, driver };
  } catch (err: unknown) {
    void err;
    return undefined;
  }
}

export function readActivePairsFromDisk(opsDir?: string): ActivePairFact[] {
  const dir = opsDir || path.join(process.env.CHORUS_HOME || path.join(os.homedir(), '.chorus'), 'ops', 'active-pairs');
  if (!existsSync(dir)) return [];

  const pairs: ActivePairFact[] = [];
  try {
    const files = readdirSync(dir);
    for (const f of files) {
      const pair = parsePairFile(dir, f);
      if (pair) pairs.push(pair);
    }
  } catch (readDirErr: unknown) {
    void readDirErr;
  }
  return pairs;
}
/* eslint-enable security/detect-non-literal-fs-filename */

export class ActorStateService {
  private cache: ActorStateApiResponse = {
    derivedAt: new Date().toISOString(),
    actors: [],
    unmeasured: [],
  };
  private lastKnownPrincipals: Array<string | PrincipalInfo> = [];
  private timer?: NodeJS.Timeout;
  private readonly pollIntervalMs: number;
  private readonly factFetcher: () => Promise<ActorFactInput>;

  constructor(factFetcher: () => Promise<ActorFactInput>, pollIntervalMs = 15000) {
    this.factFetcher = factFetcher;
    this.pollIntervalMs = pollIntervalMs;
  }

  private computeRefreshResult(input: ActorFactInput, fallbackDerivedAt: string): ActorStateApiResponse {
    const derivedAt = input.now ? input.now.toISOString() : fallbackDerivedAt;
    const rolesUnmeasured = Boolean(
      input.unmeasured?.some((u) => u.source.includes('role')) ||
      input.principals.length === 0
    );

    if (input.principals.length > 0) {
      this.lastKnownPrincipals = input.principals;
    }

    if (!rolesUnmeasured) {
      const actors = deriveActorState(input);
      this.cache = { derivedAt, actors, unmeasured: input.unmeasured || [] };
      return this.cache;
    }

    const unmeasuredList = input.unmeasured && input.unmeasured.length > 0
      ? input.unmeasured
      : [{ source: 'athena:roles', reason: 'roles door unreadable' }];

    if (this.lastKnownPrincipals.length > 0) {
      const actors = deriveActorState({
        ...input,
        principals: this.lastKnownPrincipals,
        unmeasured: unmeasuredList,
      });
      this.cache = { derivedAt, actors, unmeasured: unmeasuredList };
    } else {
      this.cache = { derivedAt, actors: [], unmeasured: unmeasuredList };
    }
    return this.cache;
  }

  private buildFallbackResult(reason: string, fallbackDerivedAt: string): ActorStateApiResponse {
    const unmeasuredList = [{ source: 'athena:roles', reason }];
    if (this.lastKnownPrincipals.length > 0) {
      const actors = deriveActorState({
        principals: this.lastKnownPrincipals,
        unmeasured: unmeasuredList,
      });
      this.cache = { derivedAt: fallbackDerivedAt, actors, unmeasured: unmeasuredList };
    } else {
      this.cache = { derivedAt: fallbackDerivedAt, actors: [], unmeasured: unmeasuredList };
    }
    return this.cache;
  }

  async refresh(): Promise<ActorStateApiResponse> {
    const fallbackDerivedAt = new Date().toISOString();
    try {
      const input = await this.factFetcher();
      return this.computeRefreshResult(input, fallbackDerivedAt);
    } catch (err: unknown) {
      const reason = err instanceof Error ? err.message : String(err);
      return this.buildFallbackResult(reason, fallbackDerivedAt);
    }
  }

  getActorStates(): ActorStateApiResponse {
    return this.cache;
  }

  start(): void {
    if (this.timer) return;
    void this.refresh();
    this.timer = setInterval(() => {
      void this.refresh();
    }, this.pollIntervalMs);
    this.timer.unref();
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }
}

async function fetchRolesFact(
  athenaBase: string,
  unmeasured: Array<{ source: string; reason: string }>
): Promise<PrincipalInfo[]> {
  try {
    const res = await fetch(`${athenaBase}/v1/roles/roles`, { signal: AbortSignal.timeout(2000) });
    if (!res.ok) {
      unmeasured.push({ source: 'athena:roles', reason: `HTTP ${res.status}` });
      return [];
    }
    const json = (await res.json()) as { data?: Array<{ name: string; label?: string; roleKind?: string }> };
    return (json.data || []).filter(isTeamRole).map((r) => {
      const cleanLabel = r.label ? r.label.replace(/^role-/, '') : '';
      return {
        principal: roleToPrincipal(r.name),
        name: cleanLabel ? formatPrincipalName(cleanLabel) : formatPrincipalName(r.name),
        roleKind: r.roleKind || 'agent',
      };
    });
  } catch (e: unknown) {
    unmeasured.push({ source: 'athena:roles', reason: e instanceof Error ? e.message : String(e) });
    return [];
  }
}

interface RawSessionRow {
  name: string;
  ownedBy?: string;
  channel?: string;
  expiresAt?: string;
  lastSeenAt?: string;
  endedAt?: string;
  sessionState?: string;
}

async function fetchSessionsFact(
  athenaBase: string,
  unmeasured: Array<{ source: string; reason: string }>
): Promise<SessionInfo[]> {
  try {
    const res = await fetch(`${athenaBase}/v1/identity/sessions?limit=5000`, { signal: AbortSignal.timeout(2000) });
    if (!res.ok) {
      unmeasured.push({ source: 'athena:identity:sessions', reason: `HTTP ${res.status}` });
      return [];
    }
    const json = (await res.json()) as { data?: RawSessionRow[] };
    return (json.data || []).map((s) => ({
      id: s.name,
      principal: s.ownedBy || '',
      channel: s.channel || 'pane',
      expiresAt: s.expiresAt || '',
      lastSeenAt: s.lastSeenAt || '',
      endedAt: s.endedAt,
      sessionState: s.sessionState,
    }));
  } catch (e: unknown) {
    unmeasured.push({ source: 'athena:identity:sessions', reason: e instanceof Error ? e.message : String(e) });
    return [];
  }
}

async function fetchWipCardsFact(
  apiBase: string,
  unmeasured: Array<{ source: string; reason: string }>
): Promise<WipCardFact[]> {
  try {
    const res = await fetch(`${apiBase}/api/chorus/context/board/wip`, { signal: AbortSignal.timeout(2000) });
    if (!res.ok) {
      unmeasured.push({ source: 'chorus:board:wip', reason: `HTTP ${res.status}` });
      return [];
    }
    const json = (await res.json()) as { data?: { cards?: Array<{ id: number; title?: string; owner: string }> } };
    return (json.data?.cards || []).map((c) => ({
      cardId: c.id,
      title: c.title,
      owner: c.owner.toLowerCase(),
    }));
  } catch (e: unknown) {
    unmeasured.push({ source: 'chorus:board:wip', reason: e instanceof Error ? e.message : String(e) });
    return [];
  }
}

interface LokiQueryResult {
  data?: {
    result?: Array<{
      metric?: { role?: string };
      value?: [number, string];
    }>;
  };
}

function normalizeLokiRole(rawRole?: string): string {
  const r = rawRole ? String(rawRole).toLowerCase() : '';
  return r === 'bridge' ? 'jeff' : r;
}

function populateLoki5m(targetMap: Map<string, ActionFact>, json5m: LokiQueryResult): void {
  for (const item of json5m.data?.result || []) {
    const role = normalizeLokiRole(item.metric?.role);
    if (!role) continue;
    const existing = targetMap.get(role) || {};
    const count = parseInt(item.value?.[1] || '0', 10);
    existing.recent5mCount = (existing.recent5mCount ?? 0) + count;
    targetMap.set(role, existing);
  }
}

function populateLoki30m(
  targetMap: Map<string, ActionFact>,
  json30m: LokiQueryResult
): void {
  for (const item of json30m.data?.result || []) {
    const role = normalizeLokiRole(item.metric?.role);
    if (!role) continue;
    const existing = targetMap.get(role) || {};
    const count30m = parseInt(item.value?.[1] || '0', 10);
    existing.recent30mCount = (existing.recent30mCount ?? 0) + count30m;
    targetMap.set(role, existing);
  }
}

// #4566 — the regex rides a backtick LogQL string: inside a double-quoted LogQL string
// `\.` is an invalid escape and Loki answers 400, which read every actor's activity as unmeasured.
export function lokiActionQuery(window: string): string {
  return `sum by (role) (count_over_time({job="platform-chorus"}[${window}] |~ \`agent\\.action|"event":"jeff\\.input\\.delivered"\`))`;
}

async function fetchLokiActionFacts(
  lokiBase: string,
  unmeasured: Array<{ source: string; reason: string }>
): Promise<Map<string, ActionFact>> {
  const result = new Map<string, ActionFact>();
  const q5m = encodeURIComponent(lokiActionQuery('5m'));
  const q30m = encodeURIComponent(lokiActionQuery('30m'));

  try {
    const [res5m, res30m] = await Promise.all([
      fetch(`${lokiBase}/loki/api/v1/query?query=${q5m}`, { signal: AbortSignal.timeout(3000) }),
      fetch(`${lokiBase}/loki/api/v1/query?query=${q30m}`, { signal: AbortSignal.timeout(3500) }),
    ]);

    if (res5m.ok && res30m.ok) {
      const json5m = (await res5m.json()) as LokiQueryResult;
      const json30m = (await res30m.json()) as LokiQueryResult;
      populateLoki5m(result, json5m);
      populateLoki30m(result, json30m);
    } else {
      unmeasured.push({ source: 'loki:activity', reason: `HTTP ${res5m.status || res30m.status}` });
    }
  } catch (e: unknown) {
    unmeasured.push({ source: 'loki:activity', reason: e instanceof Error ? e.message : String(e) });
  }

  return result;
}

function resolveSpineEventRole(ev: { role?: string; event?: string }): string {
  if (ev.event === 'jeff.input.delivered' || ev.event === 'jeff.input.surfaced') {
    return 'jeff';
  }
  const role = ev.role ? String(ev.role).toLowerCase() : '';
  if (!role || role === 'system' || role === 'pulse' || role === 'bridge') return '';
  return role;
}

function recordRecentAction(existing: ActionFact, timestamp: string, actionDesc: string): void {
  const recent = existing.recentActions || [];
  if (!recent.some((r) => r.timestamp === timestamp && r.action === actionDesc)) {
    recent.push({ action: actionDesc, timestamp });
    recent.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
    existing.recentActions = recent.slice(0, 5);
  }
}

function processSpineTailLine(line: string, targetMap: Map<string, ActionFact>): void {
  try {
    const ev = JSON.parse(line) as { role?: string; event?: string; timestamp?: string; action?: string };
    const role = resolveSpineEventRole(ev);
    if (!role) return;
    const tsMs = ev.timestamp ? new Date(ev.timestamp).getTime() : 0;
    if (tsMs <= 0 || !ev.timestamp) return;

    const existing = targetMap.get(role) || {};
    const cur = existing.lastActionAt ? new Date(existing.lastActionAt).getTime() : 0;
    if (tsMs > cur) {
      existing.lastActionAt = ev.timestamp;
    }
    const ageMs = Date.now() - tsMs;
    if (ageMs <= FIVE_MINUTES_MS) {
      existing.recent5mCount = (existing.recent5mCount ?? 0) + 1;
    }
    if (ageMs <= THIRTY_MINUTES_MS) {
      existing.recent30mCount = (existing.recent30mCount ?? 0) + 1;
    }

    recordRecentAction(existing, ev.timestamp, ev.action || ev.event || 'action');

    targetMap.set(role, existing);
  } catch (jsonErr: unknown) {
    void jsonErr;
  }
}

async function appendSpineTailActions(
  targetMap: Map<string, ActionFact>,
  readTail: () => Promise<string>
): Promise<void> {
  try {
    const rawTail = await readTail();
    const lines = rawTail.split('\n').filter(Boolean);
    for (const line of lines) {
      processSpineTailLine(line, targetMap);
    }
  } catch (tailErr: unknown) {
    void tailErr;
  }
}

export async function fetchDaguCardSteps(
  daguBase: string,
  wipCards: WipCardFact[],
  unmeasured: Array<{ source: string; reason: string }>
): Promise<Record<number, string>> {
  const steps: Record<number, string> = {};
  if (wipCards.length === 0) return steps;
  try {
    const res = await fetch(`${daguBase}/api/v1/dag-runs?name=cicd`, { signal: AbortSignal.timeout(2000) });
    if (!res.ok) {
      for (const c of wipCards) steps[c.cardId] = 'coding';
      return steps;
    }
    const json = (await res.json()) as { dagRuns?: Array<{ params?: string; dagRunId: string; statusLabel?: string }> };
    const dagRuns = json.dagRuns || [];

    await Promise.all(
      wipCards.map(async (card) => {
        // eslint-disable-next-line security/detect-non-literal-regexp
        const cardPattern = new RegExp(`(?:^|\\s)CARD="?${card.cardId}"?(?:\\s|$)`);
        const matchingRun = dagRuns.find((r) => r.params && cardPattern.test(r.params));
        if (!matchingRun) {
          steps[card.cardId] = 'coding';
          return;
        }
        try {
          const detailRes = await fetch(`${daguBase}/api/v1/dags/cicd/dag-runs/${matchingRun.dagRunId}`, { signal: AbortSignal.timeout(1500) });
          if (detailRes.ok) {
            const detailJson = (await detailRes.json()) as {
              dagRun?: {
                nodes?: Array<{ step: { name: string }; statusLabel: string }>;
                statusLabel?: string;
              };
            };
            const nodes = detailJson.dagRun?.nodes || [];
            const activeNode = nodes.find((n) => n.statusLabel === 'running') ||
              nodes.find((n) => n.statusLabel === 'waiting') ||
              nodes.find((n) => n.statusLabel === 'failed');
            if (activeNode) {
              steps[card.cardId] = activeNode.step.name.replace(/^skill-/, '').replace(/-pipeline$/, '');
            } else {
              steps[card.cardId] = matchingRun.statusLabel === 'succeeded' ? 'landed' : (matchingRun.statusLabel || 'coding');
            }
          } else {
            steps[card.cardId] = 'coding';
          }
        } catch {
          steps[card.cardId] = 'coding';
        }
      })
    );
  } catch (e: unknown) {
    unmeasured.push({ source: 'dagu:runs', reason: e instanceof Error ? e.message : String(e) });
    for (const c of wipCards) steps[c.cardId] = 'coding';
  }
  return steps;
}

export function buildLiveActorFactFetcher(options: {
  store: {
    getOwedNudges: () => Array<{ from: string; to: string; expects: 'reply' | 'decision' | 'action'; sentAt: string }>;
    queryMessages?: (opts: { type?: string; limit?: number }) => Array<{ from: string; to: string; createdAt?: string; created_at?: string }>;
  };
  readTail: () => Promise<string>;
  athenaBaseUrl?: string;
  chorusApiBaseUrl?: string;
  lokiBaseUrl?: string;
  daguBaseUrl?: string;
  opsDir?: string;
}): () => Promise<ActorFactInput> {
  const athenaBase = options.athenaBaseUrl || process.env.ATHENA_MAKE_URL || 'http://localhost:3360';
  const apiBase = options.chorusApiBaseUrl || process.env.CHORUS_API_URL || 'http://localhost:3340';
  const lokiBase = options.lokiBaseUrl || process.env.LOKI_URL || 'http://localhost:3102';
  const daguBase = options.daguBaseUrl || process.env.DAGU_URL || 'http://127.0.0.1:8526';

  return async (): Promise<ActorFactInput> => {
    const unmeasured: Array<{ source: string; reason: string }> = [];
    const now = new Date();

    const [principals, sessions, wipCards, actionMap] = await Promise.all([
      fetchRolesFact(athenaBase, unmeasured),
      fetchSessionsFact(athenaBase, unmeasured),
      fetchWipCardsFact(apiBase, unmeasured),
      fetchLokiActionFacts(lokiBase, unmeasured),
    ]);

    const cardSteps = await fetchDaguCardSteps(daguBase, wipCards, unmeasured);
    const activePairs = readActivePairsFromDisk(options.opsDir);
    await appendSpineTailActions(actionMap, options.readTail);

    const actionsRecord: Record<string, ActionFact> = Object.fromEntries(actionMap.entries());
    const rawOwed = options.store.getOwedNudges();
    const recentNudges = typeof options.store.queryMessages === 'function'
      ? options.store.queryMessages({ type: 'nudge', limit: 200 })
      : [];
    const owedNudges: OwedNudgeFact[] = rawOwed.map((n) => {
      const sentMs = parseTimestampMs(n.sentAt);
      const reply = recentNudges.find((m) => {
        const mCreated = (m as { createdAt?: string; created_at?: string }).createdAt
          || (m as { created_at?: string }).created_at;
        return (
          principalToRole(m.from) === principalToRole(n.to) &&
          principalToRole(m.to) === principalToRole(n.from) &&
          parseTimestampMs(mCreated) >= sentMs
        );
      });
      const replyCreated = reply
        ? ((reply as { createdAt?: string; created_at?: string }).createdAt || (reply as { created_at?: string }).created_at)
        : undefined;
      return replyCreated ? { ...n, repliedAt: replyCreated } : n;
    });

    return {
      now,
      principals,
      activePairs,
      sessions,
      actions: actionsRecord,
      wipCards,
      cardSteps,
      owedNudges,
      unmeasured: unmeasured.length > 0 ? unmeasured : undefined,
    };
  };
}
