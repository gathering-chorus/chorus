/**
 * derive-role-state tests (#2193 wave 4).
 *
 * Pure-function-ish derivation: given a role, WIP cards, and recent commits,
 * produce the inferred state record pulse.rs already consumes.
 */

import { deriveRoleState, type DeriveDeps } from '../src/derive-role-state';

function depsWith(
  opts: { wipCards?: Array<{ id: number; owner: string }>; recentCommits?: Array<{ sha: string; role: string; card?: number; ts: number }>; now?: number } = {},
): DeriveDeps {
  return {
    listWipCards: () => opts.wipCards ?? [],
    recentCommitsByRole: () => opts.recentCommits ?? [],
    now: () => opts.now ?? 1_000_000,
  };
}

describe('deriveRoleState', () => {
  it('role with one WIP card → building card=<id>', () => {
    const r = deriveRoleState('kade', depsWith({ wipCards: [{ id: 2193, owner: 'Kade' }] }));
    expect(r.state).toBe('building');
    expect(r.card).toBe(2193);
    expect(r.source).toBe('inferred');
    expect(r.role).toBe('kade');
  });

  it('role with no WIP but recent commits → building with card from most-recent commit', () => {
    const r = deriveRoleState('kade', depsWith({
      recentCommits: [
        { sha: 'abc123', role: 'kade', card: 2193, ts: 999_990 },
        { sha: 'def456', role: 'kade', card: 2188, ts: 999_500 },
      ],
    }));
    expect(r.state).toBe('building');
    expect(r.card).toBe(2193);
    expect(r.recent_commits?.length).toBe(2);
  });

  it('role with no WIP and no recent commits → idle, no card', () => {
    const r = deriveRoleState('kade', depsWith({}));
    expect(r.state).toBe('idle');
    expect(r.card).toBeUndefined();
  });

  it('multiple WIP cards → building but card=null, multi_wip flag true', () => {
    const r = deriveRoleState('kade', depsWith({
      wipCards: [{ id: 2193, owner: 'Kade' }, { id: 2188, owner: 'Kade' }],
    }));
    expect(r.state).toBe('building');
    expect(r.card).toBeUndefined();
    expect(r.multi_wip).toBe(true);
  });

  it('case-insensitive owner matching', () => {
    const r = deriveRoleState('kade', depsWith({ wipCards: [{ id: 2193, owner: 'kade' }] }));
    expect(r.card).toBe(2193);
  });

  it('ignores WIP cards owned by other roles', () => {
    const r = deriveRoleState('kade', depsWith({
      wipCards: [{ id: 100, owner: 'Wren' }, { id: 101, owner: 'Silas' }],
    }));
    expect(r.state).toBe('idle');
    expect(r.card).toBeUndefined();
  });

  it('output contains ts from now() and role', () => {
    const r = deriveRoleState('kade', depsWith({ now: 1_234_567 }));
    expect(r.ts).toBe(1_234_567);
    expect(r.role).toBe('kade');
  });

  it('WIP takes precedence over recent commits for card selection', () => {
    const r = deriveRoleState('kade', depsWith({
      wipCards: [{ id: 2193, owner: 'Kade' }],
      recentCommits: [{ sha: 'abc123', role: 'kade', card: 9999, ts: 999_990 }],
    }));
    expect(r.card).toBe(2193);
  });
});
