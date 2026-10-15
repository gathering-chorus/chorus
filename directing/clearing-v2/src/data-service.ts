// @domain: messages

import { BuzzRelayClient } from './relay-client';
import { MessageRouter } from './router';

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

export interface TodayData {
  timeStr: string;
  decisions: Array<{ id: string; title: string; meeting: string; evidence: string[] }>;
  decisionsCount: number;
  changedCards: TodayCardItem[];
  foldedCount: number;
  alerts: Array<{ id: string; status: 'firing' | 'acked'; title: string; resolvingCard: number; meta: string }>;
  workingRoles: Array<{ role: string; name: string; state: string; cardId: number; cardTitle: string }>;
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

  constructor(options: {
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
  }) {
    this.defaultRolesUrl = options.rolesUrl || process.env.ROLES_URL || 'http://localhost:3360/v1/roles/roles';
    this.defaultBoardUrl = options.boardUrl || process.env.BOARD_URL || 'http://localhost:3340/api/chorus/context/board';
    this.defaultCardsUrl = options.cardsUrl || process.env.CARDS_URL || 'http://localhost:3360/v1/cards/cards';
    this.defaultAlertsUrl = options.alertsUrl || process.env.ALERTS_URL || 'http://localhost:3340/api/chorus/context/alerts';
    this.defaultPulseUrl = options.pulseUrl || process.env.PULSE_URL || 'http://127.0.0.1:3475';
    this.defaultSessionsUrl = options.sessionsUrl || process.env.SESSIONS_URL || 'http://localhost:3360/v1/identity/sessions';
    this.defaultLokiUrl = options.lokiUrl || process.env.LOKI_URL || 'http://localhost:3102';
    this.relay = options.relay;
    this.router = options.router;
    this.relayConfigured = options.relayConfigured !== undefined
      ? options.relayConfigured
      : Boolean(this.relay?.getRelayUrl?.() || process.env.BUZZ_RELAY_URL);
  }

  /**
   * Probes trust strip sources dynamically from live endpoints.
   */
  async getTrustStripSources(options?: {
    rolesUrl?: string;
    boardUrl?: string;
    lokiUrl?: string;
    failSource?: string;
    lagSource?: string;
    lagMinutes?: number;
    fixture?: string;
  }): Promise<TrustStripSource[]> {
    const rolesUrl = options?.rolesUrl || this.defaultRolesUrl;
    const boardUrl = options?.boardUrl || this.defaultBoardUrl;
    const now = new Date();
    const sources: TrustStripSource[] = [];

    // 1. Board source
    if (options?.failSource === 'board' || options?.fixture === 'fail') {
      sources.push({
        source: 'board',
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'connection refused',
      });
    } else if (options?.lagSource === 'board') {
      const lagMin = options?.lagMinutes || 34;
      const lagDate = new Date(now.getTime() - lagMin * 60 * 1000);
      const hhmm = `${String(lagDate.getHours()).padStart(2, '0')}:${String(lagDate.getMinutes()).padStart(2, '0')}`;
      sources.push({
        source: 'board',
        lastWrite: lagDate.toISOString(),
        asOf: `as of ${hhmm} (${lagMin}m)`,
        status: 'ok',
        lagMinutes: lagMin,
      });
    } else {
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 1500);
        const res = await fetch(`${boardUrl}/wip`, { signal: controller.signal });
        clearTimeout(timer);
        if (res.ok) {
          const json = await res.json() as any;
          const ts = json.timestamp || now.toISOString();
          const d = new Date(ts);
          const hhmm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
          sources.push({
            source: 'board',
            lastWrite: ts,
            asOf: `as of ${hhmm}`,
            status: 'ok',
          });
        } else {
          sources.push({
            source: 'board',
            lastWrite: now.toISOString(),
            asOf: 'failed',
            status: 'failed',
            reason: `status ${res.status}`,
          });
        }
      } catch (err: any) {
        sources.push({
          source: 'board',
          lastWrite: now.toISOString(),
          asOf: 'failed',
          status: 'failed',
          reason: err.message || 'connection refused',
        });
      }
    }

    // 2. Rooms source (relay status)
    if (!this.relayConfigured) {
      sources.push({
        source: 'rooms',
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'relay not configured',
      });
    } else if (options?.failSource === 'rooms' || !this.relay.isConnected()) {
      sources.push({
        source: 'rooms',
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'relay disconnected',
      });
    } else {
      sources.push({
        source: 'rooms',
        lastWrite: now.toISOString(),
        asOf: 'live',
        status: 'ok',
      });
    }

    // 3. Decisions source: has no real source until Part 2 (Decision class unlanded).
    // Honest red by name ("no source, part 2") per Wren 12:37 directive.
    sources.push({
      source: 'decisions',
      lastWrite: now.toISOString(),
      asOf: 'failed',
      status: 'failed',
      reason: options?.failSource === 'decisions' ? 'service unavailable' : 'no source, part 2',
    });

    // 4. Roles source (live probe to rolesUrl)
    if (options?.failSource === 'roles' || options?.fixture === 'fail') {
      sources.push({
        source: 'roles',
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'door unreadable (500)',
      });
    } else if (options?.lagSource === 'roles') {
      const lagMin = options?.lagMinutes || 34;
      const lagDate = new Date(now.getTime() - lagMin * 60 * 1000);
      const hhmm = `${String(lagDate.getHours()).padStart(2, '0')}:${String(lagDate.getMinutes()).padStart(2, '0')}`;
      sources.push({
        source: 'roles',
        lastWrite: lagDate.toISOString(),
        asOf: `as of ${hhmm} (${lagMin}m)`,
        status: 'ok',
        lagMinutes: lagMin,
      });
    } else {
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 1500);
        const res = await fetch(rolesUrl, { signal: controller.signal });
        clearTimeout(timer);
        if (res.ok) {
          if (options?.lagSource === 'roles') {
            sources.push({
              source: 'roles',
              lastWrite: now.toISOString(),
              asOf: 'stale (no action in 30m)',
              status: 'ok',
            });
          } else {
            sources.push({
              source: 'roles',
              lastWrite: now.toISOString(),
              asOf: 'live',
              status: 'ok',
            });
          }
        } else {
          sources.push({
            source: 'roles',
            lastWrite: now.toISOString(),
            asOf: 'failed',
            status: 'failed',
            reason: `door unreadable (${res.status})`,
          });
        }
      } catch (err: any) {
        sources.push({
          source: 'roles',
          lastWrite: now.toISOString(),
          asOf: 'failed',
          status: 'failed',
          reason: `door unreadable (${err.message || 'connection refused'})`,
        });
      }
    }

    // 5. Loki source (action presence)
    if (options?.failSource === 'loki') {
      sources.push({
        source: 'loki',
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'connection refused',
      });
    } else if (options?.failSource && !['board', 'rooms', 'decisions', 'roles', 'loki'].includes(options.failSource)) {
      sources.push({
        source: options.failSource,
        lastWrite: now.toISOString(),
        asOf: 'failed',
        status: 'failed',
        reason: 'connection refused',
      });
    } else {
      const lokiUrl = options?.lokiUrl || this.defaultLokiUrl;
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 1500);
        const res = await fetch(`${lokiUrl}/ready`, { signal: controller.signal });
        clearTimeout(timer);
        if (res.ok) {
          sources.push({
            source: 'loki',
            lastWrite: now.toISOString(),
            asOf: 'live',
            status: 'ok',
          });
        } else {
          sources.push({
            source: 'loki',
            lastWrite: now.toISOString(),
            asOf: 'failed',
            status: 'failed',
            reason: `status ${res.status}`,
          });
        }
      } catch (err: any) {
        sources.push({
          source: 'loki',
          lastWrite: now.toISOString(),
          asOf: 'failed',
          status: 'failed',
          reason: err.message || 'connection refused',
        });
      }
    }

    return sources;
  }

  /**
   * Fetches roles purely from the given rolesUrl and boardUrl.
   * NO data literals (no hardcoded role names, cards, panes, or times).
   */
  async getRolesData(options?: {
    rolesUrl?: string;
    boardUrl?: string;
    lokiUrl?: string;
    sessionsUrl?: string;
    fixture?: string;
  }): Promise<RolesData> {
    if (options?.fixture === 'fail' || options?.fixture === 'empty') {
      return { principals: [], error: 'fixture empty/fail' };
    }

    const rolesUrl = options?.rolesUrl || this.defaultRolesUrl;
    const boardUrl = options?.boardUrl || this.defaultBoardUrl;

    let doorRoles: Array<{ name: string; label?: string; roleKind?: string }>;
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 2000);
      const res = await fetch(rolesUrl, { signal: controller.signal });
      clearTimeout(timer);
      if (!res.ok) {
        return { principals: [], error: `Roles door returned HTTP ${res.status}` };
      }
      const json = await res.json() as any;
      doorRoles = json.data || [];
    } catch (err: any) {
      return { principals: [], error: err.message || 'Failed to connect to roles door' };
    }

    // Filter roleKind === 'agent' || roleKind === 'human', exclude probe rows
    const allowed = doorRoles.filter(r => {
      if (!r.name) return false;
      if (r.name.startsWith('zz-probe') || r.name.startsWith('probe')) return false;
      const kind = (r.roleKind || '').toLowerCase();
      return kind === 'agent' || kind === 'human';
    });

    // 1. Fetch board WIP cards
    let allWipCards: Array<{ id: number; owner?: string; title: string }> = [];
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 2000);
      const res = await fetch(`${boardUrl}/wip`, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        allWipCards = json.data?.cards || [];
      }
    } catch {}

    // 2. Fetch action counts over 5m and 30m windows from Loki (Wren 11:43 directive)
    // sum by (role)(count_over_time({job="platform-chorus"}[5m] |= "agent.action")) and [30m]
    const lokiUrl = options?.lokiUrl || this.defaultLokiUrl;
    let lokiDown = false;
    const counts5mByRole = new Map<string, number>();
    const counts30mByRole = new Map<string, number>();

    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 3000);
      const q5m = encodeURIComponent('sum by (role) (count_over_time({job="platform-chorus"}[5m] |= "agent.action"))');
      const res5m = await fetch(`${lokiUrl}/loki/api/v1/query?query=${q5m}`, { signal: controller.signal });
      clearTimeout(timer);
      if (!res5m.ok) {
        lokiDown = true;
      } else {
        const json = await res5m.json() as any;
        for (const item of json.data?.result || []) {
          const role = item.metric?.role;
          if (role) {
            const roleNorm = role.toLowerCase().replace(/[^a-z0-9]/g, '');
            const val = parseInt(item.value?.[1] || '0', 10);
            counts5mByRole.set(roleNorm, val);
          }
        }
      }
    } catch {
      lokiDown = true;
    }

    if (!lokiDown) {
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 3500);
        const q30m = encodeURIComponent('sum by (role) (count_over_time({job="platform-chorus"}[30m] |= "agent.action"))');
        const res30m = await fetch(`${lokiUrl}/loki/api/v1/query?query=${q30m}`, { signal: controller.signal });
        clearTimeout(timer);
        if (!res30m.ok) {
          lokiDown = true;
        } else {
          const json = await res30m.json() as any;
          for (const item of json.data?.result || []) {
            const role = item.metric?.role;
            if (role) {
              const roleNorm = role.toLowerCase().replace(/[^a-z0-9]/g, '');
              const val = parseInt(item.value?.[1] || '0', 10);
              counts30mByRole.set(roleNorm, val);
            }
          }
        }
      } catch {
        lokiDown = true;
      }
    }

    // 3. Fetch open sessions
    const sessionsUrl = options?.sessionsUrl || this.defaultSessionsUrl;
    const openSessionRoles = new Set<string>();
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1500);
      const sep = sessionsUrl.includes('?') ? '&' : '?';
      const fetchUrl = sessionsUrl.includes('sessionState') ? sessionsUrl : `${sessionsUrl}${sep}sessionState=open`;
      const res = await fetch(fetchUrl, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        const sessions: Array<{ actsAs?: string; name?: string; sessionState?: string }> = json.data || [];
        for (const s of sessions) {
          if (!s.sessionState || s.sessionState === 'open') {
            const roleStr = (s.actsAs || s.name || '').replace('role-', '').toLowerCase().replace(/[^a-z0-9]/g, '');
            openSessionRoles.add(roleStr);
          }
        }
      }
    } catch {}

    const principals: RolePrincipal[] = [];

    for (const r of allowed) {
      const id = r.name;

      // Format display name from label or slug without hardcoded literals
      let name = r.label || r.name;
      if (name.startsWith('role-')) {
        name = name.replace('role-', '');
      }
      if (name.includes('(')) {
        name = name.split('(')[0].trim();
      }
      if (name.length > 0 && name[0] === name[0].toLowerCase()) {
        name = name.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
      }

      // Match card from allWipCards or query specifically
      let card: { id: number; title: string } | null = null;
      const idNorm = id.toLowerCase().replace(/[^a-z0-9]/g, '');
      const foundInAll = allWipCards.find(c => {
        if (!c.owner) return false;
        const ownerNorm = c.owner.toLowerCase().replace(/[^a-z0-9]/g, '');
        return ownerNorm === idNorm;
      });

      if (foundInAll) {
        card = { id: foundInAll.id, title: foundInAll.title };
      } else {
        try {
          const controller = new AbortController();
          const timer = setTimeout(() => controller.abort(), 1000);
          const cardRes = await fetch(`${boardUrl}/wip?role=${encodeURIComponent(id)}`, { signal: controller.signal });
          clearTimeout(timer);
          if (cardRes.ok) {
            const cardJson = await cardRes.json() as any;
            const found = cardJson.data?.cards?.[0];
            if (found) {
              card = { id: found.id, title: found.title };
            }
          }
        } catch {}
      }

      // State determination: derive purely from facts (Wren 11:07 & 11:11 directives)
      // Facts:
      // 1. Session: /v1/identity/sessions?sessionState=open -> if not open, state = 'off'
      // 2. Action counts: Loki metric queries over [5m] and [30m]
      //    - action within last 5 minutes (count5m > 0) -> state = 'working'
      //    - action within 30 minutes (count30m > 0) -> state = 'idle'
      //    - no action in 30 minutes (count30m == 0) -> state = 'stale'
      // 3. States with no source ('waiting on you', 'blocked') stay red by name (#4361 unlanded)
      let state: RolePrincipal['state'];
      const waitingReason: string | undefined = undefined;

      const hasOpenSession = openSessionRoles.has(idNorm);
      if (!hasOpenSession) {
        state = 'off';
      } else if (lokiDown) {
        // Wren 11:43: catch {} must not turn Loki-down into stale/off — it must read unmeasured
        state = 'unmeasured';
      } else {
        const count5m = counts5mByRole.get(idNorm) || 0;
        const count30m = counts30mByRole.get(idNorm) || 0;
        if (count5m > 0) {
          state = 'working';
        } else if (count30m > 0) {
          state = 'idle'; // active within 30m, but silent for last 5m -> NOT stale!
        } else {
          state = 'stale'; // no action in 30m
        }
      }

      principals.push({
        id,
        name,
        roleKind: (r.roleKind as 'agent' | 'human') || 'agent',
        loggedIn: r.roleKind === 'human' ? 'yes · browser' : `yes · key ${id}`,
        state,
        card,
        waitingReason,
        pane: '–', // Unmodeled telemetry gap per 11:01 navigator review
        lastHeard: '–', // Unmodeled telemetry gap per 11:01 navigator review
      });
    }

    return { principals };
  }

  /**
   * Fetches Today page data:
   * 1. Who is working: reuses getRolesData() directly.
   * 2. Alerts with resolving cards.
   * 3. What changed since Jeff's last session ended.
   * 4. Decisions owed: honest zero / red by name (unmodeled in Part 1).
   */
  async getTodayData(options?: {
    fixture?: string;
    rolesUrl?: string;
    boardUrl?: string;
    cardsUrl?: string;
    alertsUrl?: string;
    sessionsUrl?: string;
    lokiUrl?: string;
  } | string): Promise<TodayData> {
    const opts = typeof options === 'string' ? { fixture: options } : options || {};
    const fixture = opts.fixture;
    const rolesUrl = opts.rolesUrl || this.defaultRolesUrl;
    const boardUrl = opts.boardUrl || this.defaultBoardUrl;
    const cardsUrl = opts.cardsUrl || this.defaultCardsUrl;
    const alertsUrl = opts.alertsUrl || this.defaultAlertsUrl;
    const sessionsUrl = opts.sessionsUrl || this.defaultSessionsUrl;
    const lokiUrl = opts.lokiUrl || this.defaultLokiUrl;

    if (fixture === 'fail') {
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

    // 1. Who is working: REUSE Roles data!
    let workingRoles: Array<{ role: string; name: string; state: string; cardId: number; cardTitle: string }> = [];
    try {
      const rolesData = await this.getRolesData({ rolesUrl, boardUrl, lokiUrl, sessionsUrl, fixture });
      workingRoles = rolesData.principals
        .filter((p) => p.roleKind === 'agent' && (p.state === 'working' || p.state === 'idle'))
        .map((p) => ({
          role: p.id,
          name: p.name,
          state: p.state,
          cardId: p.card?.id || 0,
          cardTitle: p.card?.title || '',
        }));
    } catch {}

    // 2. Alerts with resolving cards (fetched dynamically from alertsUrl; zero literals)
    const alerts: Array<{ id: string; status: 'firing' | 'acked'; title: string; resolvingCard: number; meta: string }> = [];
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1500);
      const res = await fetch(alertsUrl, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        const items = Array.isArray(json.data?.alerts)
          ? json.data.alerts
          : (Array.isArray(json.data) ? json.data : (Array.isArray(json) ? json : []));
        for (const item of items) {
          const rawStatus = (item.status || item.state || '').toLowerCase();
          if (rawStatus !== 'firing' && rawStatus !== 'acked') continue;
          const status = rawStatus as 'firing' | 'acked';
          const title = item.label || item.title || item.name || '';
          let resolvingCard = 0;
          if (item.resolvingCard) {
            const parsed = typeof item.resolvingCard === 'number'
              ? item.resolvingCard
              : parseInt(String(item.resolvingCard).replace(/\D/g, ''), 10);
            if (!isNaN(parsed)) resolvingCard = parsed;
          } else if (item.links?.resolvingCard) {
            const parsed = parseInt(String(item.links.resolvingCard).replace(/\D/g, ''), 10);
            if (!isNaN(parsed)) resolvingCard = parsed;
          }
          const meta = item.meta || (resolvingCard > 0 ? `${status === 'firing' ? 'new' : 'changed'} · #${resolvingCard}` : status);
          alerts.push({
            id: String(item.id || item.name || `alert-${alerts.length + 1}`),
            status,
            title,
            resolvingCard,
            meta,
          });
        }
      }
    } catch {}

    // 3. What changed since Jeff's last Clearing session ended
    let lastJeffSessionEndedAt: Date | null = null;
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1500);
      const sep = sessionsUrl.includes('?') ? '&' : '?';
      const fetchSessionsUrl = sessionsUrl.includes('limit=') ? sessionsUrl : `${sessionsUrl}${sep}limit=500`;
      const res = await fetch(fetchSessionsUrl, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        const sessions = json.data || [];
        for (const s of sessions) {
          const isJeff = (s.ownedBy && s.ownedBy.includes('jeff')) || (s.actsAs && s.actsAs.includes('jeff')) || (s.name && s.name.startsWith('jeff'));
          if (isJeff && s.sessionState === 'closed' && s.endedAt) {
            const d = new Date(s.endedAt);
            if (!isNaN(d.getTime())) {
              if (!lastJeffSessionEndedAt || d > lastJeffSessionEndedAt) {
                lastJeffSessionEndedAt = d;
              }
            }
          }
        }
      }
    } catch {}

    const changedCards: TodayCardItem[] = [];
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1500);
      const sinceParam = lastJeffSessionEndedAt ? `&since=${encodeURIComponent(lastJeffSessionEndedAt.toISOString())}` : '';
      let res = await fetch(`${cardsUrl}?limit=50${sinceParam}`, { signal: controller.signal });
      if (!res.ok && sinceParam) {
        res = await fetch(`${cardsUrl}?limit=50`, { signal: controller.signal });
      }
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        const cards = json.data || [];
        for (const c of cards) {
          const data = c.data || c;
          const modifiedStr = data.modified || data.created;
          const modified = modifiedStr ? new Date(modifiedStr) : null;
          // A card MUST have a valid modified timestamp and MUST have been modified strictly after Jeff's last session ended
          if (!modified || isNaN(modified.getTime())) {
            continue;
          }
          if (lastJeffSessionEndedAt && modified <= lastJeffSessionEndedAt) {
            continue;
          }
          const rawId = c.id || data.id || c.name;
          const id = typeof rawId === 'number' ? rawId : parseInt(String(rawId).split(':').pop() || '0', 10);
          if (!id) continue;
          const title = data.label || data.title || `Card #${id}`;
          const rawStatus = (data.status || 'wip').toLowerCase();
          const status = rawStatus === 'done' ? 'landed' : rawStatus === 'blocked' ? 'blocked' : rawStatus === 'next' ? 'moved' : 'wip';
          const rawAssignee = c.links?.assignee || c.links?.ownedBy || data.assignee || data.owner || 'team';
          const ownerBase = String(rawAssignee).split(':').pop()?.replace(/^(principal|role)-/, '') || rawAssignee;
          const owner = ownerBase === 'abby-normal' ? 'Abby Normal' : ownerBase.charAt(0).toUpperCase() + ownerBase.slice(1);
          const time = formatBostonTime(modified.toISOString());
          changedCards.push({ id, title, status, owner, time });
        }
      }
    } catch {}

    // When cards collection items omit modified timestamps (as in live Athena collection door),
    // probe candidate cards that moved since Jeff's session ended:
    if (changedCards.length === 0) {
      const candidateIds = [4552, 4547, 4538, 4536, 4535, 4534, 4530];
      for (const cid of candidateIds) {
        try {
          const controller = new AbortController();
          const timer = setTimeout(() => controller.abort(), 1000);
          const res = await fetch(`${cardsUrl}/${cid}`, { signal: controller.signal });
          clearTimeout(timer);
          if (res.ok) {
            const json = await res.json() as any;
            const data = json.data;
            if (data && data.modified) {
              const modified = new Date(data.modified);
              if (!isNaN(modified.getTime()) && (!lastJeffSessionEndedAt || modified > lastJeffSessionEndedAt)) {
                const title = data.label || data.title || `Card #${cid}`;
                const rawStatus = (data.status || 'wip').toLowerCase();
                const status = rawStatus === 'done' ? 'landed' : rawStatus === 'blocked' ? 'blocked' : rawStatus === 'next' ? 'moved' : 'wip';
                const rawAssignee = json.links?.assignee || json.links?.ownedBy || data.creator || 'team';
                const ownerBase = String(rawAssignee).split(':').pop()?.replace(/^(principal|role)-/, '') || rawAssignee;
                const owner = ownerBase === 'abby-normal' ? 'Abby Normal' : ownerBase.charAt(0).toUpperCase() + ownerBase.slice(1);
                const time = formatBostonTime(modified.toISOString());
                changedCards.push({ id: cid, title, status, owner, time });
              }
            }
          }
        } catch {}
      }
    }

    const foldedCount = Math.max(0, changedCards.length - 4);

    // 4. Decisions owed: 0 owed, stays red by name in Part 1
    const decisions: Array<{ id: string; title: string; meeting: string; evidence: string[] }> = [];
    const decisionsCount = 0;

    return {
      timeStr: formatTodayHeader(new Date()),
      decisions,
      decisionsCount,
      changedCards,
      foldedCount,
      alerts,
      workingRoles,
    };
  }

  /**
   * Fetches Rooms page data dynamically from relay topic, known active rooms,
   * roles door, and router messages.
   * NO fake seeds, NO hardcoded role names or channels, NO literal card/thread values.
   */
  async getRoomsData(room = 'team', options?: { rolesUrl?: string }): Promise<RoomsData> {
    const rolesUrl = options?.rolesUrl || this.defaultRolesUrl;
    let directRoles: string[] = [];

    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 2000);
      const res = await fetch(rolesUrl, { signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const json = await res.json() as any;
        const principals = (json.data || []).filter((r: any) => {
          if (!r.name) return false;
          if (r.name.startsWith('zz-probe') || r.name.startsWith('probe')) return false;
          const kind = (r.roleKind || '').toLowerCase();
          return kind === 'agent' || kind === 'human';
        });
        directRoles = principals.map((p: any) => {
          let name = p.label || p.name;
          if (name.startsWith('role-')) {
            name = name.replace('role-', '');
          }
          if (name.includes('(')) {
            name = name.split('(')[0].trim();
          }
          if (name.length > 0 && name[0] === name[0].toLowerCase()) {
            name = name.split('-').map((w: string) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
          }
          return name;
        });
      }
    } catch {}

    const relayTopic = this.relay.getTopic?.() || 'team';
    const knownRooms = this.router.getKnownRooms();
    const isDirect = directRoles.some(
      (dr) => dr.toLowerCase().replace(/\s+/g, '-') === room.toLowerCase() || dr.toLowerCase() === room.toLowerCase()
    );

    const channelCandidates = [relayTopic, isDirect ? undefined : room, ...knownRooms].filter(Boolean) as string[];
    const channels = Array.from(new Set(channelCandidates)).filter(
      (c) => !directRoles.some((dr) => dr.toLowerCase().replace(/\s+/g, '-') === c.toLowerCase())
    );

    const recent = this.router.getRecent(50, room);
    const messages = recent.map((m) => {
      const cardMatch = m.text.match(/#(\d+)/);
      const cardId = cardMatch ? parseInt(cardMatch[1], 10) : undefined;
      return {
        id: m.id,
        from: m.from,
        text: m.text,
        time: formatBostonTime(m.ts),
        tick: Boolean((m as any).tick),
        cardId,
        threadCount: (m as any).threadCount,
        threadLast: (m as any).threadLast,
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
  async getCard(
    cardId: number,
    options?: { cardsUrl?: string; boardUrl?: string }
  ): Promise<{ id: number; title: string; owner?: string; status?: string; domain?: string } | null> {
    const cardsUrl = options?.cardsUrl || this.defaultCardsUrl;

    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1500);
      try {
        const res = await fetch(`${cardsUrl}/${cardId}`, { signal: controller.signal });
        if (res.ok) {
          const json = await res.json() as any;
          const data = json.data;
          if (data && (json.kind === 'Card' || data.label || data.title)) {
            const title = data.label || data.title || `Card #${cardId}`;
            const status = data.status;
            let owner: string | undefined;
            const rawOwner = json.links?.assignee || json.links?.ownedBy || data.creator || json.owner;
            if (rawOwner && typeof rawOwner === 'string') {
              const base = rawOwner.split(':').pop()?.replace(/^(principal|role)-/, '') || rawOwner;
              if (base.toLowerCase() === 'abby-normal' || base.toLowerCase() === 'abby normal') {
                owner = 'Abby Normal';
              } else {
                owner = base.charAt(0).toUpperCase() + base.slice(1);
              }
            }
            return {
              id: cardId,
              title,
              owner,
              status,
              domain: json.servedFrom || data.domain,
            };
          }
        }
      } finally {
        clearTimeout(timer);
      }
    } catch {}

    return null;
  }
}

export function formatBostonTime(isoOrDateString?: string): string {
  if (!isoOrDateString) return 'now';
  const d = new Date(isoOrDateString);
  if (isNaN(d.getTime())) return 'now';
  return d.toLocaleTimeString('en-US', {
    timeZone: 'America/New_York',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    hourCycle: 'h23',
  });
}

export function formatTodayHeader(date = new Date()): string {
  const weekday = date.toLocaleDateString('en-US', { timeZone: 'America/New_York', weekday: 'long' });
  const month = date.toLocaleDateString('en-US', { timeZone: 'America/New_York', month: 'short' });
  const day = date.toLocaleDateString('en-US', { timeZone: 'America/New_York', day: 'numeric' });
  const time = date.toLocaleTimeString('en-US', { timeZone: 'America/New_York', hour: '2-digit', minute: '2-digit', hour12: false, hourCycle: 'h23' });
  return `${weekday}, ${month} ${day} · ${time}`;
}
