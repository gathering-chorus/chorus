/**
 * GET /api/chorus/context/roles (#2234 Step 3).
 *
 * Answers: "What is each role doing right now?" Returns every declared role
 * with state / card / gemba / last-activity / last-event, in canonical
 * envelope shape.
 *
 * Sources:
 *   - /tmp/claude-team-scan/<role>-declared.json — role-state writer output
 *   - platform/logs/chorus.log — spine events, tailed for last activity
 *
 * DI surface: all external reads go through `deps.readState` and
 * `deps.tailSpine`. Tests inject stubs; production wires to fs.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export const KNOWN_ROLES = ['silas', 'wren', 'kade'] as const;
export type RoleName = (typeof KNOWN_ROLES)[number];

export interface RoleStateRecord {
  role: string;
  state: string;
  card?: number | null;
  gemba?: string | null;
  detail?: string | null;
}

export interface SpineEventRecord {
  timestamp: string;
  role: string;
  event: string;
}

export interface InferredRoleRecord {
  card?: number | null;
  state?: string | null;
  ts?: number | null;
  wip_count?: number | null;
  recent_commit_count?: number | null;
}

export interface ContextRolesDeps {
  sparql: StampSparqlClient;
  /** Returns the role's declared state record or null when missing/unparseable. */
  readState: (role: string) => RoleStateRecord | null;
  /** Returns the most recent spine event for the given role, or null. */
  tailSpine: (role: string) => SpineEventRecord | null;
  /** #2193 AC5: returns inferred state (from derive-role-state), null if missing. */
  readInferred?: (role: string) => InferredRoleRecord | null;
  /** Override in tests so timestamp and `lastActivity` gap behavior are deterministic. */
  now?: () => Date;
}

/** Roles older than 15 min without a spine event are marked stale. */
const STALE_THRESHOLD_MS = 15 * 60 * 1000;
/** Inferred records older than this are considered stale. Mirrors pulse.rs INFERRED_TTL_SECS. */
const INFERRED_STALE_MS = 5 * 60 * 1000;

export interface DriftState {
  divergent: boolean;
  inferred_stale: boolean;
  card_declared: number | null;
  card_inferred: number | null;
}

export interface ContextRolesRow {
  name: string;
  state: string;
  card: number | null;
  gemba: string | null;
  lastActivity: string | null;
  lastEvent: string | null;
  /** true when lastActivity is absent or older than STALE_THRESHOLD_MS */
  stale: boolean;
  /** #2193 AC5: derived state from observed work (WIP + commits). */
  derived_state: { state: string | null; card: number | null; wip_count: number | null; recent_commit_count: number | null } | null;
  /** #2193 AC5: drift diagnostic — are declared and derived coherent? */
  drift_state: DriftState;
}

export interface ContextRolesResponse {
  status: number;
  body: ContextEnvelope<{ roles: ContextRolesRow[] }>;
}

function computeDrift(declaredCard: number | null, inferred: InferredRoleRecord | null, nowMs: number): DriftState {
  const inferredCard = inferred?.card ?? null;
  const inferredTs = inferred?.ts ?? null;
  const inferredStale = inferred === null
    || inferredTs === null
    || (nowMs - inferredTs * 1000) > INFERRED_STALE_MS;
  const divergent = declaredCard !== null
    && inferredCard !== null
    && !inferredStale
    && declaredCard !== inferredCard;
  return {
    divergent,
    inferred_stale: inferredStale,
    card_declared: declaredCard,
    card_inferred: inferredCard,
  };
}

function shapeDerivedState(inferred: InferredRoleRecord | null): ContextRolesRow['derived_state'] {
  if (!inferred) return null;
  return {
    state: inferred.state ?? null,
    card: inferred.card ?? null,
    wip_count: inferred.wip_count ?? null,
    recent_commit_count: inferred.recent_commit_count ?? null,
  };
}

function shapeRoleRow(deps: ContextRolesDeps, name: string, nowMs: number): ContextRolesRow {
  const st = deps.readState(name);
  const sp = deps.tailSpine(name);
  const inferred = deps.readInferred?.(name) ?? null;
  const lastActivity = sp?.timestamp ?? null;
  const stale = lastActivity === null
    || nowMs - new Date(lastActivity).getTime() > STALE_THRESHOLD_MS;
  const declaredCard = st?.card ?? null;
  return {
    name,
    state: st?.state ?? 'unknown',
    card: declaredCard,
    gemba: st?.gemba ?? null,
    lastActivity,
    lastEvent: sp?.event ?? null,
    stale,
    derived_state: shapeDerivedState(inferred),
    drift_state: computeDrift(declaredCard, inferred, nowMs),
  };
}

export async function fetchContextRoles(
  deps: ContextRolesDeps,
  sourceUrl: string,
): Promise<ContextRolesResponse> {
  const header = await stampHeader(deps.sparql, null);
  const nowMs = (deps.now?.() ?? new Date()).getTime();
  const rows: ContextRolesRow[] = KNOWN_ROLES.map((name) => shapeRoleRow(deps, name, nowMs));
  return { status: 200, body: buildEnvelope(header, sourceUrl, { roles: rows }) };
}
