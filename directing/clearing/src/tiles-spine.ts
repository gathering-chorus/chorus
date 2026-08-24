/**
 * #3882 — role-tile state as a PROJECTION of the spine.
 *
 * Jeff, 2026-08-15: "if messages and streams are right then role state
 * largely follows." The spine now reliably carries every role's activity
 * (observer digests, reply.emitted/published, werk.phase per pipeline step),
 * so the tile stops trusting hand-declared files for anything observation can
 * answer. Declared state survives only where observation cannot infer intent:
 * blocked and observing. Everything else is a fact about events landing.
 *
 * One clock (#3880 coordination): every age is event-timestamp vs a single
 * `now` passed in by the caller — no mixing of file mtimes, declared ts, and
 * wall reads that produced the bogus tile ages.
 */

export const SPINE_ACTIVE_WINDOW_SECS = 120;

/** Roles whose events count as team activity. Machinery attribution (pulse,
 *  bridge, system, unknown, heartbeat-probe…) must never light a tile —
 *  that is exactly the "looks alive while dead" failure inverted. */
export const AGENT_ROLES = new Set(['jeff', 'wren', 'silas', 'kade']);

export interface SpineActivity {
  ageSecs: number;
  kind: string;
}

/**
 * #2725 (2026-08-24) — LIVENESS OF THE PROCESS IS NOT ACTIVITY OF THE ROLE.
 *
 * The tile and the streams pane read the same file and still disagreed: the
 * tile called silas active "8s ago" while his pane had been silent 48 minutes
 * (caught live by the #3976 reconciliation flow). Both windows were right; the
 * ACCEPT RULES differed. `system.heartbeat` is emitted by the running process
 * on a timer whether or not the role does anything, so counting it as activity
 * turns the tile into a liveness lamp that always reads "working".
 *
 * These kinds are excluded from "the role did something". Everything else —
 * turns, tool calls, replies, digests, werk phases, nudges — still counts, so
 * a role that is thinking rather than shelling out does not fall to idle.
 */
const HEARTBEAT_KINDS = new Set([
  'system.heartbeat',
  'heartbeat.probe',
  'pair.heartbeat.silence',
]);

/** Latest role-attributed spine event per role, ages against ONE `now`. */
export function latestSpineActivity(
  lines: string[],
  now: number,
): Record<string, SpineActivity> {
  const out: Record<string, SpineActivity> = {};
  for (const line of lines) {
    let e: { timestamp?: string; role?: string; event?: string };
    try {
      e = JSON.parse(line);
    } catch {
      continue;
    }
    const role = e.role ?? '';
    if (!AGENT_ROLES.has(role) || !e.timestamp || !e.event) continue;
    if (HEARTBEAT_KINDS.has(e.event)) continue;
    const ts = Date.parse(e.timestamp);
    if (Number.isNaN(ts)) continue;
    const ageSecs = Math.max(0, Math.floor((now - ts) / 1000));
    const prev = out[role];
    if (!prev || ageSecs < prev.ageSecs) {
      out[role] = { ageSecs, kind: e.event };
    }
  }
  return out;
}

export interface ProjectedState {
  state: string;
  ageSecs: number | null;
}

/**
 * The projection rule. Activity within the window → building (or the
 * declared intent-state if it is one observation cannot infer). Stale or
 * absent activity → idle/unknown with the SPINE age — never a remembered
 * declaration's word for it.
 */
export function projectRoleState(input: {
  declared: string;
  lastEventAgeSecs: number | null;
  lastEventKind: string | null;
  now: number;
}): ProjectedState {
  if (input.lastEventAgeSecs === null) {
    return { state: 'unknown', ageSecs: null };
  }
  const active = input.lastEventAgeSecs < SPINE_ACTIVE_WINDOW_SECS;
  if (!active) {
    return { state: 'idle', ageSecs: input.lastEventAgeSecs };
  }
  if (input.declared === 'blocked' || input.declared === 'observing') {
    return { state: input.declared, ageSecs: input.lastEventAgeSecs };
  }
  return { state: 'building', ageSecs: input.lastEventAgeSecs };
}
