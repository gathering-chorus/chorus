// @domain: messages

import { BuzzRelayClient } from './relay-client';
import { MessageRouter } from './router';
import { ClearingMessage } from './types';
import { RolesData, TodayData, RoomsData, DoorRolesResponse, TrustStripSource, TodayCardItem } from './sources/models';
import { fetchJson, failureReason } from './sources/http';
import { formatBostonTime, formatBostonWhen, formatTodayHeader, isPrincipalRow, roleDisplayName } from './sources/format';
import { buildTrustStripSources, TrustStripOptions } from './sources/trust-strip';
import { fetchAlerts } from './sources/today';
import { fetchActorState, fetchChangesOvernight, toActorRow, toChangedCard, unmeasuredText } from './sources/pulse';
import { fetchDrawerCard, DrawerCard } from './sources/card';
import { fetchCardPipeline, CardPipelineStatus } from './sources/pipeline';

export type { TrustStripSource, ActorRow, RolesData, TodayCardItem, TodayData, RoomsData } from './sources/models';
export type { DrawerCard } from './sources/card';
export type { CardPipelineStatus } from './sources/pipeline';
export { formatBostonTime, formatTodayHeader } from './sources/format';

export interface DataServiceOptions {
  rolesUrl?: string;
  pulseUrl?: string;
  cardsUrl?: string;
  athenaCardUrl?: string;
  alertsUrl?: string;
  runWerkStatus?: (cardId: number) => Promise<string>;
  relay: BuzzRelayClient;
  router: MessageRouter;
  relayConfigured?: boolean;
}

export interface RolesOptions {
  pulseUrl?: string;
}

export interface TodayOptions extends RolesOptions {
  alertsUrl?: string;
}

type RecentMessage = ClearingMessage & { threadCount?: number; threadLast?: string };

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
  private defaultCardsUrl: string;
  private defaultAthenaCardUrl: string;
  private defaultAlertsUrl: string;
  private defaultPulseUrl: string;
  private runWerkStatus?: (cardId: number) => Promise<string>;
  private relay: BuzzRelayClient;
  private router: MessageRouter;
  private relayConfigured: boolean;

  constructor(options: DataServiceOptions) {
    this.defaultRolesUrl = firstSet(options.rolesUrl, process.env.ROLES_URL, 'http://localhost:3360/v1/roles/roles');
    this.defaultCardsUrl = firstSet(options.cardsUrl, process.env.CARDS_URL, 'http://localhost:3360/v1/cards/cards');
    this.defaultAthenaCardUrl = firstSet(options.athenaCardUrl, process.env.ATHENA_CARD_URL, 'http://localhost:3340/api/athena/card');
    this.defaultAlertsUrl = firstSet(options.alertsUrl, process.env.ALERTS_URL, 'http://localhost:3340/api/chorus/context/alerts');
    this.defaultPulseUrl = firstSet(options.pulseUrl, process.env.PULSE_URL, 'http://127.0.0.1:3475');
    this.runWerkStatus = options.runWerkStatus;
    this.relay = options.relay;
    this.router = options.router;
    this.relayConfigured = options.relayConfigured ?? Boolean(this.relay.getRelayUrl() || process.env.BUZZ_RELAY_URL);
  }

  /** Trust strip: v2's own relay chip, then each reading pulse's /api/readings returns. */
  async getTrustStripSources(options?: TrustStripOptions): Promise<TrustStripSource[]> {
    return buildTrustStripSources({
      pulseUrl: this.defaultPulseUrl,
      relayConfigured: this.relayConfigured,
      relayConnected: () => this.relay.isConnected(),
    }, options);
  }

  /** Roles: every actor exactly as pulse's /api/actor-state reports it; v2 derives no state. */
  async getRolesData(options?: RolesOptions): Promise<RolesData> {
    const read = await fetchActorState(options?.pulseUrl || this.defaultPulseUrl);
    if (!read.ok) return { actors: [], unmeasured: read.reason };
    return {
      actors: (read.value.actors || []).map(toActorRow),
      unmeasured: unmeasuredText(read.value.unmeasured),
    };
  }

  /**
   * Today page data:
   * 1. Working now: the actors from pulse's actor-state (same read as Roles).
   * 2. Alerts with resolving cards.
   * 3. Changed overnight: the cards pulse's changes-overnight returns, as sent.
   * 4. Decisions owed: honest red by name (unmodeled in Part 1).
   */
  async getTodayData(options?: TodayOptions): Promise<TodayData> {
    const pulseUrl = options?.pulseUrl || this.defaultPulseUrl;
    const [roles, alertsRead, changes] = await Promise.all([
      this.getRolesData({ pulseUrl }),
      fetchAlerts(options?.alertsUrl || this.defaultAlertsUrl),
      fetchChangesOvernight(pulseUrl),
    ]);
    // #4574 — unreadable alerts source reads unmeasured with the reason, never empty or zero.
    const alerts = alertsRead.ok ? alertsRead.value : [];
    const alertsUnmeasured = alertsRead.ok ? undefined : `alerts API unreadable (${failureReason(alertsRead, 'connection refused')})`;
    const changedCards: TodayCardItem[] = changes.ok ? (changes.value.cards || []).map(toChangedCard) : [];
    const watermark = changes.ok && changes.value.watermark ? formatBostonWhen(changes.value.watermark) : undefined;

    return {
      timeStr: formatTodayHeader(new Date()),
      decisions: [],
      decisionsCount: 0,
      changedCards,
      changedUnmeasured: changes.ok ? unmeasuredText(changes.value.unmeasured) : changes.reason,
      watermark,
      foldedCount: Math.max(0, changedCards.length - 4),
      alerts,
      alertsUnmeasured,
      // display filter: Working now leaves out actors pulse says are away or off
      workingRoles: roles.actors.filter((a) => a.state !== 'off' && a.state !== 'away'),
      workingUnmeasured: roles.unmeasured,
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
   * Fetches card metadata from Athena card detail or Athena cards door for drawer inspection.
   */
  async getCard(cardId: number, options?: { cardsUrl?: string; athenaCardUrl?: string }): Promise<DrawerCard | null> {
    return fetchDrawerCard(options?.cardsUrl || this.defaultCardsUrl, cardId, {
      athenaCardUrl: options?.athenaCardUrl || this.defaultAthenaCardUrl,
    });
  }

  /**
   * #4598: Fetches card pipeline state exactly as /cws <card> prints it.
   */
  async getCardPipeline(cardId: number, options?: { cwsBin?: string; runCws?: (cardId: number) => Promise<string> }): Promise<CardPipelineStatus> {
    const runCws = options?.runCws || this.runWerkStatus;
    return fetchCardPipeline(cardId, { ...options, runCws });
  }
}
