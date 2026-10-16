// @domain: messages

import { BuzzRelayClient } from './relay-client';
import { MessageRouter } from './router';
import { ClearingMessage } from './types';
import { RolesData, TodayData, RoomsData, TodayWorkingRole, DoorRolesResponse, TrustStripSource } from './sources/models';
import { fetchJson, errorMessage } from './sources/http';
import { formatBostonTime, formatTodayHeader, isPrincipalRow, roleDisplayName } from './sources/format';
import { buildTrustStripSources, TrustStripOptions } from './sources/trust-strip';
import { fetchRolesData } from './sources/roles';
import { fetchAlerts, fetchLastJeffSessionEnd, fetchChangedCards } from './sources/today';
import { fetchDrawerCard, DrawerCard } from './sources/card';

export type { TrustStripSource, RolePrincipal, RolesData, TodayCardItem, TodayData, RoomsData } from './sources/models';
export { formatBostonTime, formatTodayHeader } from './sources/format';

export interface DataServiceOptions {
  rolesUrl?: string;
  boardUrl?: string;
  pulseUrl?: string;
  sessionsUrl?: string;
  lokiUrl?: string;
  cardsUrl?: string;
  alertsUrl?: string;
  relay: BuzzRelayClient;
  router: MessageRouter;
  relayConfigured?: boolean;
}

export interface RolesOptions {
  rolesUrl?: string;
  boardUrl?: string;
  lokiUrl?: string;
  sessionsUrl?: string;
  fixture?: string;
}

export interface TodayOptions extends RolesOptions {
  cardsUrl?: string;
  alertsUrl?: string;
}

type RecentMessage = ClearingMessage & { threadCount?: number; threadLast?: string };

function emptyToday(): TodayData {
  return {
    timeStr: formatTodayHeader(new Date()),
    decisions: [],
    decisionsCount: 0,
    changedCards: [],
    foldedCount: 0,
    alerts: [],
    workingRoles: [],
  };
}

function slug(name: string): string {
  return name.toLowerCase().replace(/\s+/g, '-');
}

/** Display names of agent/human principals from the roles door; none when it is unreadable. */
async function fetchDirectRoles(rolesUrl: string): Promise<string[]> {
  const r = await fetchJson<DoorRolesResponse | null>(rolesUrl, 2000);
  if (!r.ok) return [];
  return (r.value?.data || []).filter(isPrincipalRow).map(roleDisplayName);
}

/** First non-empty of: explicit option, environment value, default. */
function firstSet(explicit: string | undefined, fromEnv: string | undefined, fallback: string): string {
  return explicit || fromEnv || fallback;
}

export class DataService {
  private defaultRolesUrl: string;
  private defaultBoardUrl: string;
  private defaultCardsUrl: string;
  private defaultAlertsUrl: string;
  private defaultPulseUrl: string;
  private defaultSessionsUrl: string;
  private defaultLokiUrl: string;
  private relay: BuzzRelayClient;
  private router: MessageRouter;
  private relayConfigured: boolean;

  constructor(options: DataServiceOptions) {
    this.defaultRolesUrl = firstSet(options.rolesUrl, process.env.ROLES_URL, 'http://localhost:3360/v1/roles/roles');
    this.defaultBoardUrl = firstSet(options.boardUrl, process.env.BOARD_URL, 'http://localhost:3340/api/chorus/context/board');
    this.defaultCardsUrl = firstSet(options.cardsUrl, process.env.CARDS_URL, 'http://localhost:3360/v1/cards/cards');
    this.defaultAlertsUrl = firstSet(options.alertsUrl, process.env.ALERTS_URL, 'http://localhost:3340/api/chorus/context/alerts');
    this.defaultPulseUrl = firstSet(options.pulseUrl, process.env.PULSE_URL, 'http://127.0.0.1:3475');
    this.defaultSessionsUrl = firstSet(options.sessionsUrl, process.env.SESSIONS_URL, 'http://localhost:3360/v1/identity/sessions');
    this.defaultLokiUrl = firstSet(options.lokiUrl, process.env.LOKI_URL, 'http://localhost:3102');
    this.relay = options.relay;
    this.router = options.router;
    this.relayConfigured = options.relayConfigured ?? Boolean(this.relay.getRelayUrl() || process.env.BUZZ_RELAY_URL);
  }

  /**
   * Probes trust strip sources dynamically from live endpoints.
   */
  async getTrustStripSources(options?: TrustStripOptions): Promise<TrustStripSource[]> {
    return buildTrustStripSources({
      rolesUrl: this.defaultRolesUrl,
      boardUrl: this.defaultBoardUrl,
      lokiUrl: this.defaultLokiUrl,
      relayConfigured: this.relayConfigured,
      relayConnected: () => this.relay.isConnected(),
    }, options);
  }

  /**
   * Fetches roles purely from the given rolesUrl and boardUrl.
   * NO data literals (no hardcoded role names, cards, panes, or times).
   */
  async getRolesData(options?: RolesOptions): Promise<RolesData> {
    if (options?.fixture === 'fail' || options?.fixture === 'empty') {
      return { principals: [], error: 'fixture empty/fail' };
    }
    return fetchRolesData({
      rolesUrl: options?.rolesUrl || this.defaultRolesUrl,
      boardUrl: options?.boardUrl || this.defaultBoardUrl,
      lokiUrl: options?.lokiUrl || this.defaultLokiUrl,
      sessionsUrl: options?.sessionsUrl || this.defaultSessionsUrl,
    });
  }

  /** Who is working: agents whose derived state is working or idle (reuses getRolesData). */
  private async getWorkingRoles(options: RolesOptions): Promise<{ roles: TodayWorkingRole[]; error?: string }> {
    const rolesData = await this.getRolesData(options).catch((err: unknown): RolesData => ({
      principals: [],
      error: errorMessage(err) || 'roles unreadable',
    }));
    if (rolesData.error) return { roles: [], error: `roles unreadable (${rolesData.error})` };
    if (rolesData.unmeasured) return { roles: [], error: rolesData.unmeasured };
    const roles = rolesData.principals
      .filter((p) => p.roleKind === 'agent' && (p.state === 'working' || p.state === 'idle'))
      .map((p) => ({
        role: p.id,
        name: p.name,
        state: p.state,
        cardId: p.card?.id || 0,
        cardTitle: p.card?.title || '',
      }));
    return { roles };
  }

  /**
   * Fetches Today page data:
   * 1. Who is working: reuses getRolesData() directly.
   * 2. Alerts with resolving cards.
   * 3. What changed since Jeff's last session ended.
   * 4. Decisions owed: honest zero / red by name (unmodeled in Part 1).
   */
  async getTodayData(options?: TodayOptions | string): Promise<TodayData> {
    const opts: TodayOptions = typeof options === 'string' ? { fixture: options } : options || {};
    if (opts.fixture === 'fail') return emptyToday();

    const sessionsUrl = opts.sessionsUrl || this.defaultSessionsUrl;
    const working = await this.getWorkingRoles({
      rolesUrl: opts.rolesUrl || this.defaultRolesUrl,
      boardUrl: opts.boardUrl || this.defaultBoardUrl,
      lokiUrl: opts.lokiUrl || this.defaultLokiUrl,
      sessionsUrl,
      fixture: opts.fixture,
    });

    // Unreadable alerts door -> no alerts row; unreadable sessions -> no since filter.
    const alertsRead = await fetchAlerts(opts.alertsUrl || this.defaultAlertsUrl);
    const alerts = alertsRead.ok ? alertsRead.value : [];
    const lastEnd = await fetchLastJeffSessionEnd(sessionsUrl);
    const since = lastEnd.ok ? lastEnd.value : null;
    const changed = await fetchChangedCards(opts.cardsUrl || this.defaultCardsUrl, since, opts.lokiUrl || this.defaultLokiUrl);
    const changedCards = changed.items;

    // Decisions owed: 0 owed, stays red by name in Part 1
    return {
      timeStr: formatTodayHeader(new Date()),
      decisions: [],
      decisionsCount: 0,
      changedCards,
      changedUnmeasured: changed.unmeasured,
      foldedCount: Math.max(0, changedCards.length - 4),
      alerts,
      workingRoles: working.roles,
      workingUnmeasured: working.error,
    };
  }

  /**
   * Fetches Rooms page data dynamically from relay topic, known active rooms,
   * roles door, and router messages.
   * NO fake seeds, NO hardcoded role names or channels, NO literal card/thread values.
   */
  async getRoomsData(room = 'team', options?: { rolesUrl?: string }): Promise<RoomsData> {
    const directRoles = await fetchDirectRoles(options?.rolesUrl || this.defaultRolesUrl);

    const relayTopic = this.relay.getTopic() || 'team';
    const knownRooms = this.router.getKnownRooms();
    const roomLower = room.toLowerCase();
    const isDirect = directRoles.some((dr) => slug(dr) === roomLower || dr.toLowerCase() === roomLower);

    const channelCandidates = [relayTopic, isDirect ? undefined : room, ...knownRooms].filter(Boolean) as string[];
    const channels = Array.from(new Set(channelCandidates)).filter(
      (c) => !directRoles.some((dr) => slug(dr) === c.toLowerCase())
    );

    const recent: RecentMessage[] = this.router.getRecent(50, room);
    const messages = recent.map((m) => {
      const cardMatch = m.text.match(/#(\d+)/);
      return {
        id: m.id,
        from: m.from,
        text: m.text,
        time: formatBostonTime(m.ts),
        tick: Boolean(m.tick),
        cardId: cardMatch ? parseInt(cardMatch[1], 10) : undefined,
        threadCount: m.threadCount,
        threadLast: m.threadLast,
      };
    });

    return {
      room,
      channels: channels.length > 0 ? channels : ['team'],
      directRoles,
      relayConnected: this.relay.isConnected(),
      messages,
    };
  }

  /**
   * Fetches card metadata from Athena cards door (single source) for drawer inspection.
   */
  async getCard(cardId: number, options?: { cardsUrl?: string; boardUrl?: string }): Promise<DrawerCard | null> {
    return fetchDrawerCard(options?.cardsUrl || this.defaultCardsUrl, cardId);
  }
}
