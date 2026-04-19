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

export interface ContextRolesDeps {
  sparql: StampSparqlClient;
  /** Returns the role's declared state record or null when missing/unparseable. */
  readState: (role: string) => RoleStateRecord | null;
  /** Returns the most recent spine event for the given role, or null. */
  tailSpine: (role: string) => SpineEventRecord | null;
  /** Override in tests so timestamp and `lastActivity` gap behavior are deterministic. */
  now?: () => Date;
}

/** Roles older than 15 min without a spine event are marked stale. */
const STALE_THRESHOLD_MS = 15 * 60 * 1000;

export interface ContextRolesRow {
  name: string;
  state: string;
  card: number | null;
  gemba: string | null;
  lastActivity: string | null;
  lastEvent: string | null;
  /** true when lastActivity is absent or older than STALE_THRESHOLD_MS */
  stale: boolean;
}

export interface ContextRolesResponse {
  status: number;
  body: ContextEnvelope<{ roles: ContextRolesRow[] }>;
}

export async function fetchContextRoles(
  deps: ContextRolesDeps,
  sourceUrl: string,
): Promise<ContextRolesResponse> {
  const header = await stampHeader(deps.sparql, null);
  const nowMs = (deps.now?.() ?? new Date()).getTime();
  const rows: ContextRolesRow[] = KNOWN_ROLES.map((name) => {
    const st = deps.readState(name);
    const sp = deps.tailSpine(name);
    const lastActivity = sp?.timestamp ?? null;
    const stale = lastActivity === null
      ? true
      : nowMs - new Date(lastActivity).getTime() > STALE_THRESHOLD_MS;
    return {
      name,
      state: st?.state ?? 'unknown',
      card: st?.card ?? null,
      gemba: st?.gemba ?? null,
      lastActivity,
      lastEvent: sp?.event ?? null,
      stale,
    };
  });
  const envelope = buildEnvelope(header, sourceUrl, { roles: rows });
  return { status: 200, body: envelope };
}
