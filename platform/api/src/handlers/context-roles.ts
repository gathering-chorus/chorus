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

export const KNOWN_ROLES = ['silas', 'wren', 'kade'] as const;
export type RoleName = (typeof KNOWN_ROLES)[number];

export interface ContextRolesDeps {
  sparql: StampSparqlClient;
  /** Spine lines for the role since `sinceMs` (epoch ms). May include other roles; the derivation filters. */
  readEvents: (role: string, sinceMs: number) => SpineLine[];
  /** The board's WIP cards with owners. */
  listWipCards: () => WipCardEntry[];
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
}

export interface ContextRolesResponse {
  status: number;
  body: ContextEnvelope<{ roles: ContextRolesRow[] }>;
}

function shapeRoleRow(deps: ContextRolesDeps, name: string, nowMs: number, wip: WipCardEntry[]): ContextRolesRow {
  const events = deps.readEvents(name, nowMs - DEMO_LOOKBACK_MS);
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
  };
}

export async function fetchContextRoles(
  deps: ContextRolesDeps,
  sourceUrl: string,
): Promise<ContextRolesResponse> {
  const header = await stampHeader(deps.sparql, null);
  const nowMs = (deps.now?.() ?? new Date()).getTime();
  const wip = deps.listWipCards();
  const rows: ContextRolesRow[] = KNOWN_ROLES.map((name) => shapeRoleRow(deps, name, nowMs, wip));
  return { status: 200, body: buildEnvelope(header, sourceUrl, { roles: rows }) };
}
