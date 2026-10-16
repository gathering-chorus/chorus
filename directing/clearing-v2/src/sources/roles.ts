// @domain: messages
// Roles: principals from the roles door, their WIP card from the board, state derived from
// open sessions + Loki action counts. NO data literals (no hardcoded role names, cards, panes, or times).

import {
  RolesData,
  RolePrincipal,
  DoorRole,
  DoorRolesResponse,
  BoardWipCard,
  BoardWipResponse,
  LokiQueryResponse,
  DoorSessionsResponse,
} from './models';
import { Fetched, fetchJson } from './http';
import { isPrincipalRow, normalizeId, roleDisplayName } from './format';

export interface RolesSourceUrls {
  rolesUrl: string;
  boardUrl: string;
  lokiUrl: string;
  sessionsUrl: string;
}

/** Action counts per normalized role id; null when Loki is down (state reads 'unmeasured'). */
interface ActionCounts {
  last5m: Map<string, number>;
  last30m: Map<string, number>;
}

type RoleCard = { id: number; title: string } | null;

async function fetchDoorRoles(rolesUrl: string): Promise<Fetched<DoorRole[]>> {
  const r = await fetchJson<DoorRolesResponse | null>(rolesUrl, 2000);
  if (!r.ok) {
    const message = r.status !== undefined
      ? `Roles door returned HTTP ${r.status}`
      : (r.message || 'Failed to connect to roles door');
    return { ok: false, status: r.status, message };
  }
  return { ok: true, value: r.value?.data || [] };
}

async function fetchWipCards(boardUrl: string): Promise<Fetched<BoardWipCard[]>> {
  const r = await fetchJson<BoardWipResponse | null>(`${boardUrl}/wip`, 2000);
  return r.ok ? { ok: true, value: r.value?.data?.cards || [] } : r;
}

// sum by (role)(count_over_time({job="platform-chorus"}[<window>] |= "agent.action")) (Wren 11:43 directive)
async function fetchLokiActionCounts(lokiUrl: string, window: '5m' | '30m', ms: number): Promise<Fetched<Map<string, number>>> {
  const q = encodeURIComponent(`sum by (role) (count_over_time({job="platform-chorus"}[${window}] |= "agent.action"))`);
  const r = await fetchJson<LokiQueryResponse | null>(`${lokiUrl}/loki/api/v1/query?query=${q}`, ms);
  if (!r.ok) return r;
  const counts = new Map<string, number>();
  for (const item of r.value?.data?.result || []) {
    const role = item.metric?.role;
    if (role) {
      counts.set(normalizeId(role), parseInt(item.value?.[1] || '0', 10));
    }
  }
  return { ok: true, value: counts };
}

/** Both windows, or null if either read failed (the 30m read is skipped once 5m fails). */
async function fetchActionCounts(lokiUrl: string): Promise<ActionCounts | null> {
  const last5m = await fetchLokiActionCounts(lokiUrl, '5m', 3000);
  if (!last5m.ok) return null;
  const last30m = await fetchLokiActionCounts(lokiUrl, '30m', 3500);
  if (!last30m.ok) return null;
  return { last5m: last5m.value, last30m: last30m.value };
}

async function fetchOpenSessionRoles(sessionsUrl: string): Promise<Fetched<Set<string>>> {
  const sep = sessionsUrl.includes('?') ? '&' : '?';
  const fetchUrl = sessionsUrl.includes('sessionState') ? sessionsUrl : `${sessionsUrl}${sep}sessionState=open`;
  const r = await fetchJson<DoorSessionsResponse | null>(fetchUrl, 1500);
  if (!r.ok) return r;
  const open = new Set<string>();
  for (const s of r.value?.data || []) {
    if (!s.sessionState || s.sessionState === 'open') {
      open.add(normalizeId((s.actsAs || s.name || '').replace('role-', '')));
    }
  }
  return { ok: true, value: open };
}

async function fetchRoleCard(boardUrl: string, id: string): Promise<Fetched<RoleCard>> {
  const r = await fetchJson<BoardWipResponse | null>(`${boardUrl}/wip?role=${encodeURIComponent(id)}`, 1000);
  if (!r.ok) return r;
  const found = r.value?.data?.cards?.[0];
  return { ok: true, value: found ? { id: found.id, title: found.title } : null };
}

/** The role's WIP card from the board snapshot, else asked of the board by role; no card when unreadable. */
async function resolveRoleCard(boardUrl: string, id: string, allWipCards: BoardWipCard[]): Promise<RoleCard> {
  const idNorm = normalizeId(id);
  const foundInAll = allWipCards.find((c) => Boolean(c.owner) && normalizeId(c.owner || '') === idNorm);
  if (foundInAll) return { id: foundInAll.id, title: foundInAll.title };
  const byRole = await fetchRoleCard(boardUrl, id);
  return byRole.ok ? byRole.value : null;
}

// State is derived purely from facts (Wren 11:07 & 11:11 directives):
// 1. Session not open -> 'off'
// 2. Loki down -> 'unmeasured' (Wren 11:43: Loki-down must not read as stale/off)
// 3. action in last 5m -> 'working'; in last 30m -> 'idle'; none in 30m -> 'stale'
// States with no source ('waiting on you', 'blocked') stay red by name (#4361 unlanded).
export function deriveRoleState(idNorm: string, openSessionRoles: Set<string>, counts: ActionCounts | null): RolePrincipal['state'] {
  if (!openSessionRoles.has(idNorm)) return 'off';
  if (!counts) return 'unmeasured';
  if ((counts.last5m.get(idNorm) || 0) > 0) return 'working';
  if ((counts.last30m.get(idNorm) || 0) > 0) return 'idle'; // active within 30m, silent for 5m -> NOT stale
  return 'stale';
}

export async function fetchRolesData(urls: RolesSourceUrls): Promise<RolesData> {
  const door = await fetchDoorRoles(urls.rolesUrl);
  if (!door.ok) return { principals: [], error: door.message };
  const allowed = door.value.filter(isPrincipalRow);

  // Board, sessions unreadable -> no cards / no open sessions (the role reads 'off'); Loki unreadable -> 'unmeasured'.
  const wip = await fetchWipCards(urls.boardUrl);
  const allWipCards = wip.ok ? wip.value : [];
  const counts = await fetchActionCounts(urls.lokiUrl);
  const sessions = await fetchOpenSessionRoles(urls.sessionsUrl);
  const openSessionRoles = sessions.ok ? sessions.value : new Set<string>();

  const principals: RolePrincipal[] = [];
  for (const r of allowed) {
    const id = r.name;
    const card = await resolveRoleCard(urls.boardUrl, id, allWipCards);
    principals.push({
      id,
      name: roleDisplayName(r),
      roleKind: (r.roleKind as 'agent' | 'human' | undefined) || 'agent',
      loggedIn: r.roleKind === 'human' ? 'yes · browser' : `yes · key ${id}`,
      state: deriveRoleState(normalizeId(id), openSessionRoles, counts),
      card,
      waitingReason: undefined,
      pane: '–', // Unmodeled telemetry gap per 11:01 navigator review
      lastHeard: '–', // Unmodeled telemetry gap per 11:01 navigator review
    });
  }
  return { principals };
}
