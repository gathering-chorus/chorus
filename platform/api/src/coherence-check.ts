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

export function checkCoherence(deps: CoherenceDeps): CoherenceResult {
  const roles = deps.readPulseRoles();
  const now = deps.now();
  const alarmed: string[] = [];

  for (const [role, entry] of Object.entries(roles)) {
    const drifted = Boolean(entry.divergent) || Boolean(entry.inferred_stale);
    if (!drifted) {
      // Clear tracker when state returns to coherent
      deps.writeDriftSince(role, null);
      continue;
    }

    const since = deps.readDriftSince(role);
    if (since === null) {
      // First observation of drift — start the tracker but don't alarm yet
      deps.writeDriftSince(role, now);
      continue;
    }

    const driftDuration = now - since;
    if (driftDuration < DRIFT_THRESHOLD_SECS) {
      // Tracked, but not past threshold
      continue;
    }

    const kind = entry.divergent ? 'divergent' : 'inferred_stale';
    const fields: Record<string, string> = {
      kind,
      drift_duration_secs: String(driftDuration),
      card_declared: entry.card_declared != null ? String(entry.card_declared) : '',
      card_inferred: entry.card_inferred != null ? String(entry.card_inferred) : '',
    };
    deps.emitSpineEvent('role.state.drifted', role, fields);

    const content = kind === 'divergent'
      ? `[drift] Your declared card (#${entry.card_declared ?? '?'}) and inferred card (#${entry.card_inferred ?? '?'}) disagree for ${driftDuration}s. Re-declare via role-state or commit something so observers catch up.`
      : `[drift] Your inferred state is stale for ${driftDuration}s. Derive-role-state hasn't run recently — or you're not generating observable signal (commits, WIP card moves). Nudge the system by doing a git activity.`;
    deps.sendNudge(role, content);
    alarmed.push(role);
  }

  return { alarmedRoles: alarmed };
}
