/**
 * Coherence check (#2193 AC3).
 *
 * Reads pulse.roles.* for divergent/inferred_stale flags, tracks how long
 * each role has been drifted, and when drift exceeds 60s fires
 * `role.state.drifted` on the spine and nudges the drifted role directly
 * (never Jeff — he's not the monitor).
 *
 * DI surface keeps this hermetic: pulse read, drift-since read/write,
 * spine emit, nudge send, now() — all injectable.
 */

const DRIFT_THRESHOLD_SECS = 60;

export interface RolePulseEntry {
  divergent?: boolean;
  inferred_stale?: boolean;
  card?: number;
  card_declared?: number;
  card_inferred?: number;
  state?: string;
}

export interface CoherenceDeps {
  /** Returns role → pulse entry map from /tmp/pulse-latest.json. */
  readPulseRoles: () => Record<string, RolePulseEntry>;
  /** Returns stored drift-since timestamp for role, or null if not currently tracked. */
  readDriftSince: (role: string) => number | null;
  /** Writes (or clears with null) the drift-since timestamp for role. */
  writeDriftSince: (role: string, ts: number | null) => void;
  /** Emit a spine event for the role. */
  emitSpineEvent: (event: string, role: string, fields: Record<string, string>) => void;
  /** Send a nudge to the role (NOT jeff). */
  sendNudge: (role: string, content: string) => void;
  now: () => number;
}

export interface CoherenceResult {
  alarmedRoles: string[];
}

// #2627: per-role drift handling extracted into helpers; orchestrator becomes
// a flat loop calling checkOneRole per role.

type RoleEntry = ReturnType<CoherenceDeps['readPulseRoles']>[string];

function buildDriftFields(kind: string, driftDuration: number, entry: RoleEntry): Record<string, string> {
  return {
    kind,
    drift_duration_secs: String(driftDuration),
    card_declared: entry.card_declared != null ? String(entry.card_declared) : '',
    card_inferred: entry.card_inferred != null ? String(entry.card_inferred) : '',
  };
}

function buildDriftMessage(kind: string, driftDuration: number, entry: RoleEntry): string {
  if (kind === 'divergent') {
    return `[drift] Your declared card (#${entry.card_declared ?? '?'}) and inferred card (#${entry.card_inferred ?? '?'}) disagree for ${driftDuration}s. Re-declare via role-state or commit something so observers catch up.`;
  }
  return `[drift] Your inferred state is stale for ${driftDuration}s. Derive-role-state hasn't run recently — or you're not generating observable signal (commits, WIP card moves). Nudge the system by doing a git activity.`;
}

function checkOneRole(deps: CoherenceDeps, role: string, entry: RoleEntry, now: number): boolean {
  const drifted = Boolean(entry.divergent) || Boolean(entry.inferred_stale);
  if (!drifted) {
    deps.writeDriftSince(role, null);
    return false;
  }
  const since = deps.readDriftSince(role);
  if (since === null) {
    deps.writeDriftSince(role, now);
    return false;
  }
  const driftDuration = now - since;
  if (driftDuration < DRIFT_THRESHOLD_SECS) return false;
  const kind = entry.divergent ? 'divergent' : 'inferred_stale';
  deps.emitSpineEvent('role.state.drifted', role, buildDriftFields(kind, driftDuration, entry));
  deps.sendNudge(role, buildDriftMessage(kind, driftDuration, entry));
  return true;
}

export function checkCoherence(deps: CoherenceDeps): CoherenceResult {
  const roles = deps.readPulseRoles();
  const now = deps.now();
  const alarmed: string[] = [];
  for (const [role, entry] of Object.entries(roles)) {
    if (checkOneRole(deps, role, entry, now)) alarmed.push(role);
  }
  return { alarmedRoles: alarmed };
}
