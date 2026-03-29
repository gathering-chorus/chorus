import { generateHandoffBrief } from '../src/brief';
import { WorkflowManifest, Step } from '../src/types';

function makeManifest(overrides: Partial<WorkflowManifest> = {}): WorkflowManifest {
  return {
    id: 'WF-001',
    decision: 'Migrate WordPress',
    source: 'clearing:test',
    card: 118,
    created: '2026-02-22T10:00:00Z',
    updated: '2026-02-22T10:05:00Z',
    status: 'in_progress',
    steps: [
      {
        seq: 1, role: 'silas', action: 'Write ADR', status: 'completed',
        card: null, blocked_by: [], artifacts: ['architect/adr/ADR-015.md'],
        brief: null, started_at: null, completed_at: '2026-02-22T10:05:00Z',
        notes: 'ADR-015 written and reviewed',
      },
      {
        seq: 2, role: 'kade', action: 'Implement migration', status: 'ready',
        card: null, blocked_by: [1], artifacts: [],
        brief: null, started_at: null, completed_at: null, notes: null,
      },
      {
        seq: 3, role: 'wren', action: 'Verify and roadmap', status: 'pending',
        card: null, blocked_by: [2], artifacts: [],
        brief: null, started_at: null, completed_at: null, notes: null,
      },
    ],
    verification: null,
    history: [],
    ...overrides,
  };
}

describe('generateHandoffBrief', () => {
  it('includes workflow ID and decision', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('WF-001');
    expect(brief).toContain('Migrate WordPress');
  });

  it('includes the next step action', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('Implement migration');
  });

  it('includes previous step context', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('Write ADR');
    expect(brief).toContain('ADR-015 written and reviewed');
    expect(brief).toContain('architect/adr/ADR-015.md');
  });

  it('shows "none" for missing notes', () => {
    const manifest = makeManifest();
    manifest.steps[0].notes = null;
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('**Notes**: none');
  });

  it('shows "none" for empty artifacts', () => {
    const manifest = makeManifest();
    manifest.steps[0].artifacts = [];
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('**Artifacts**: none');
  });

  it('includes progress grid with correct icons', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('[x] Step 1');
    expect(brief).toContain('[>] Step 2');
    expect(brief).toContain('[ ] Step 3');
  });

  it('includes advance command', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('workflow-ts advance WF-001');
  });

  it('identifies completed by role', () => {
    const manifest = makeManifest();
    const brief = generateHandoffBrief(manifest, manifest.steps[0], manifest.steps[1]);
    expect(brief).toContain('completed by silas');
  });
});
