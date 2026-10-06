/**
 * GET /api/chorus/context/roles (#2234 Step 3; #4028 derived-only).
 *
 * Answers: "What is each role doing right now?" — as a function of the
 * streams, recomputed on every read. There is no declared file behind this
 * endpoint any more (#4028), so there is no "unknown" and nothing to drift
 * against.
 *
 * Sources:
 *   - the spine (~/.chorus/chorus.log), last hour, filtered per role
 *   - the board's WIP cards (owner → card)
 *
 * DI surface: `deps.readEvents` and `deps.listWipCards`. Tests inject stubs;
 * production wires to the spine tail and the board cache.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';
import {
  stateFromStreams,
  DEMO_LOOKBACK_MS,
  type SpineLine,
  type WipCardEntry,
  type RoleState,
} from '../derive-role-state';

/** #4432 — an agent role as the roles door serves it. */
export interface AgentRoleRow { name: string; rolePriority?: string | number }
export type RoleName = string;

/**
 * #4432 — the agent roles, ordered by rolePriority then name, from the roles
 * door's rows (`/v1/roles/roles`, roleKind agent). Throws on a reply with no
 * data list or no agent: a tile list is never guessed.
 */
export function agentRolesFrom(body: unknown): string[] {
  const rows = (body as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) throw new Error('the roles door answered with no data list');
  const agents = (rows as Array<AgentRoleRow & { roleKind?: string }>)
    .filter((r) => r.roleKind === 'agent' && r.name)
    .sort((a, b) => (Number(a.rolePriority ?? 99) - Number(b.rolePriority ?? 99)) || a.name.localeCompare(b.name))
    .map((r) => r.name);
  if (agents.length === 0) throw new Error('the roles door lists no agent role');
  return agents;
}

/** #4432 — one open session of a principal, as its tile lists it. */
export interface OpenSession { channel: string; startedAt: string; lastSeenAt: string }

/** #4432 — a logged-in principal and its open sessions (one tile, many sessions). */
export interface LoggedInRole { name: string; sessions: OpenSession[] }

/**
 * #4432 — Jeff 2026-10-05 18:57: "the clearing needs to dynamically render
 * logged in principals." The roles (Jeff's and the agents') with an open Session (login → logout,
 * #4406: no timer ends one), in the roles door's rolePriority order. A role
 * with no open session has no tile; a login adds one, a logout removes it.
 * Jeff 2026-10-06 08:05: tiles are principals, not sessions, but he wants to
 * see each principal's sessions inside its tile "until we stabilize clearing",
 * so every row carries its open sessions, oldest first.
 */
export function loggedInRoles(sessionsBody: unknown, rolesBody: unknown): LoggedInRole[] {
  // Jeff and the agents alike (Wren 18:59: "logged in principals", not agent
  // roles): every agent or human role row, Jeff first by rolePriority 0.
  const body = rolesBody as { data?: unknown } | null;
  if (!Array.isArray(body?.data)) throw new Error('the roles door answered with no data list');
  const names = ((body as { data: unknown[] }).data as Array<AgentRoleRow & { roleKind?: string }>)
    .filter((r) => (r.roleKind === 'agent' || r.roleKind === 'human') && r.name)
    .sort((a, b) => (Number(a.rolePriority ?? 99) - Number(b.rolePriority ?? 99)) || a.name.localeCompare(b.name))
    .map((r) => r.name);
  const rows = (sessionsBody as { data?: unknown } | null)?.data;
  if (!Array.isArray(rows)) throw new Error('the sessions door answered with no data list');
  const open = new Map<string, OpenSession[]>();
  for (const r of rows as Array<Partial<OpenSession> & { sessionState?: string; ownedBy?: string }>) {
    if (r.sessionState !== 'open' || !r.ownedBy) continue;
    const role = String(r.ownedBy).replace(/^principal-/, '');
    const list = open.get(role) ?? [];
    list.push({ channel: r.channel ?? '', startedAt: r.startedAt ?? '', lastSeenAt: r.lastSeenAt ?? '' });
    open.set(role, list);
  }
  return names
    .filter((n) => open.has(n))
    .map((n) => ({ name: n, sessions: open.get(n)!.sort((a, b) => a.startedAt.localeCompare(b.startedAt)) }));
}

export interface ContextRolesDeps {
  sparql: StampSparqlClient;
  /** Spine lines for the role since `sinceMs` (epoch ms). May include other roles; the derivation filters.
   *  #4431 — may be async: production answers from the events domain's one shared reader. */
  readEvents: (role: string, sinceMs: number) => SpineLine[] | Promise<SpineLine[]>;
  /** The board's WIP cards with owners. */
  listWipCards: () => WipCardEntry[];
  /** #4432 — the logged-in principals and their open sessions; throws when the doors can't answer. */
  listAgentRoles: () => Promise<LoggedInRole[]>;
  /** Override in tests so timestamps are deterministic. */
  now?: () => Date;
}

/** Roles with no activity for this long are marked stale. */
const STALE_THRESHOLD_MS = 15 * 60 * 1000;

/** Kept for consumer-shape compatibility (#2193 readers); never divergent now — nothing to drift against. */
export interface DriftState {
  divergent: boolean;
  inferred_stale: boolean;
  card_declared: number | null;
  card_inferred: number | null;
}

export interface ContextRolesRow {
  name: string;
  /** Alias of name — consumers key off either. */
  role: string;
  state: RoleState;
  card: number | null;
  gemba: string | null;
  /** #4028 — the role.blocked detail, when blocked. */
  detail: string | null;
  lastActivity: string | null;
  lastEvent: string | null;
  /** true when lastActivity is absent or older than STALE_THRESHOLD_MS */
  stale: boolean;
  /** #4028 — always 'streams'; the provenance a reader can trust. */
  source: 'streams';
  /** Consumer shape kept from #2193; now the same derivation as `state`/`card`. */
  derived_state: { state: string | null; card: number | null; wip_count: number | null; recent_commit_count: number | null } | null;
  drift_state: DriftState;
  /** #4432 — this principal's open sessions, oldest first (one tile, N sessions). */
  sessions: OpenSession[];
}

export interface ContextRolesResponse {
  status: number;
  body: ContextEnvelope<{ roles: ContextRolesRow[] }>;
}

async function shapeRoleRow(deps: ContextRolesDeps, { name, sessions }: LoggedInRole, nowMs: number, wip: WipCardEntry[]): Promise<ContextRolesRow> {
  const events = await deps.readEvents(name, nowMs - DEMO_LOOKBACK_MS);
  const d = stateFromStreams({ role: name, events, wipCards: wip, now: nowMs });
  const stale = d.lastActivity === null
    || nowMs - new Date(d.lastActivity).getTime() > STALE_THRESHOLD_MS;
  return {
    name,
    role: name,
    state: d.state,
    card: d.card,
    gemba: d.gemba,
    detail: d.detail ?? null,
    lastActivity: d.lastActivity,
    lastEvent: d.lastEvent,
    stale,
    source: 'streams',
    derived_state: { state: d.state, card: d.card, wip_count: d.wip_count, recent_commit_count: null },
    drift_state: { divergent: false, inferred_stale: false, card_declared: null, card_inferred: d.card },
    sessions,
  };
}

export async function fetchContextRoles(
  deps: ContextRolesDeps,
  sourceUrl: string,
): Promise<ContextRolesResponse> {
  let roles: LoggedInRole[];
  try { roles = await deps.listAgentRoles(); }
  catch (e) {
    return { status: 503, body: { error: 'roles door unreadable; no tiles guessed', detail: (e as Error).message } as unknown as ContextRolesResponse['body'] };
  }
  const header = await stampHeader(deps.sparql, null);
  const nowMs = (deps.now?.() ?? new Date()).getTime();
  const wip = deps.listWipCards();
  const rows: ContextRolesRow[] = await Promise.all(roles.map((r) => shapeRoleRow(deps, r, nowMs, wip)));
  return { status: 200, body: buildEnvelope(header, sourceUrl, { roles: rows }) };
}
