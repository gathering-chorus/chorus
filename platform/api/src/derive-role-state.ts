/**
 * Derive role state from observed work (#2193 AC2).
 *
 * Reads WIP board + recent git activity and produces the inferred state
 * record that pulse.rs consumes (`/tmp/claude-team-scan/<role>-inferred.json`).
 * Declared state stays as the primary source for backward compatibility;
 * this is the comparison signal that drives coherence alarms (AC3).
 *
 * DI surface (listWipCards, recentCommitsByRole, now) keeps tests hermetic.
 * A thin CLI + LaunchAgent wraps this for periodic execution (separate wave).
 */

export interface WipCardEntry {
  id: number;
  owner: string;
}

export interface RecentCommit {
  sha: string;
  role: string;
  card?: number;
  ts: number;
}

export interface DeriveDeps {
  listWipCards: () => WipCardEntry[];
  recentCommitsByRole: (role: string) => RecentCommit[];
  now: () => number;
}

export interface InferredState {
  role: string;
  state: 'building' | 'idle';
  card?: number;
  multi_wip?: boolean;
  recent_commits?: Array<{ sha: string; card?: number; ts: number }>;
  ts: number;
  source: 'inferred';
}

export function deriveRoleState(role: string, deps: DeriveDeps): InferredState {
  const normalizedRole = role.toLowerCase();
  const myWip = deps.listWipCards().filter((c) => c.owner.toLowerCase() === normalizedRole);
  const recent = deps.recentCommitsByRole(normalizedRole);

  const base: InferredState = {
    role: normalizedRole,
    state: 'idle',
    ts: deps.now(),
    source: 'inferred',
  };

  if (myWip.length === 1) {
    return { ...base, state: 'building', card: myWip[0].id };
  }
  if (myWip.length > 1) {
    return { ...base, state: 'building', multi_wip: true };
  }

  if (recent.length > 0) {
    const sorted = [...recent].sort((a, b) => b.ts - a.ts);
    const newest = sorted[0];
    return {
      ...base,
      state: 'building',
      card: newest.card,
      recent_commits: sorted.map((c) => ({ sha: c.sha, card: c.card, ts: c.ts })),
    };
  }

  return base;
}
