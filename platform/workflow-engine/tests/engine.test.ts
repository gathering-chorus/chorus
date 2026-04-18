import { WorkflowEngine } from '../src/engine';
import { WorkflowEngineConfig, WorkflowManifest } from '../src/types';
import { mkdirSync, rmSync, existsSync, readdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

function tempConfig(): WorkflowEngineConfig {
  const base = join(tmpdir(), `workflow-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  return {
    activeDir: join(base, 'active'),
    archiveDir: join(base, 'archive'),
    briefDirs: {
      silas: join(base, 'briefs-silas'),
      kade: join(base, 'briefs-kade'),
      wren: join(base, 'briefs-wren'),
      jeff: join(base, 'briefs-jeff'),
    },
    handoffLogPath: join(base, 'logs', 'handoffs.log'),
  };
}

function cleanup(config: WorkflowEngineConfig): void {
  const base = join(config.activeDir, '..');
  try { rmSync(base, { recursive: true, force: true }); } catch { /* noop */ }
}

describe('WorkflowEngine', () => {
  let config: WorkflowEngineConfig;
  let engine: WorkflowEngine;

  beforeEach(() => {
    config = tempConfig();
    engine = new WorkflowEngine(config);
  });

  afterEach(() => {
    cleanup(config);
  });

  // --- nextId ---

  describe('nextId', () => {
    it('returns WF-001 for empty dirs', () => {
      expect(engine.nextId()).toBe('WF-001');
    });

    it('increments past existing workflows', () => {
      engine.create('First', 'wren:Do thing A');
      expect(engine.nextId()).toBe('WF-002');
    });

    it('scans both active and archive dirs', () => {
      const wf = engine.create('First', 'wren:Do thing A');
      engine.advance(wf.id); // completes single-step → archives
      expect(engine.nextId()).toBe('WF-002');
    });
  });

  // --- create ---

  describe('create', () => {
    it('creates a workflow with sequential steps', () => {
      const wf = engine.create(
        'Migrate WordPress',
        'silas:Write ADR,kade:Implement,wren:Verify',
        'clearing:test',
        118,
      );
      expect(wf.id).toBe('WF-001');
      expect(wf.decision).toBe('Migrate WordPress');
      expect(wf.source).toBe('clearing:test');
      expect(wf.card).toBe(118);
      expect(wf.status).toBe('in_progress');
      expect(wf.steps).toHaveLength(3);
      expect(wf.steps[0].status).toBe('ready');
      expect(wf.steps[1].status).toBe('pending');
      expect(wf.steps[2].status).toBe('pending');
      expect(wf.history).toHaveLength(1);
      expect(wf.history[0].event).toBe('created');
    });

    it('sets blocked_by correctly', () => {
      const wf = engine.create('Test', 'silas:A,kade:B,wren:C');
      expect(wf.steps[0].blocked_by).toEqual([]);
      expect(wf.steps[1].blocked_by).toEqual([1]);
      expect(wf.steps[2].blocked_by).toEqual([2]);
    });

    it('persists to disk', () => {
      const wf = engine.create('Persist test', 'wren:Do it');
      const files = readdirSync(config.activeDir);
      expect(files).toContain('WF-001.json');
      const loaded = JSON.parse(readFileSync(join(config.activeDir, 'WF-001.json'), 'utf-8'));
      expect(loaded.decision).toBe('Persist test');
    });

    it('rejects empty decision', () => {
      expect(() => engine.create('', 'wren:Do it')).toThrow('Decision text is required');
    });

    it('rejects empty steps', () => {
      expect(() => engine.create('Test', '')).toThrow('Steps are required');
    });

    it('rejects invalid role', () => {
      expect(() => engine.create('Test', 'bob:Do it')).toThrow('Invalid role: "bob"');
    });

    it('rejects malformed step format', () => {
      expect(() => engine.create('Test', 'no-colon')).toThrow('Invalid step format');
    });

    it('handles actions with colons', () => {
      const wf = engine.create('Test', 'silas:Deploy at 10:00');
      expect(wf.steps[0].action).toBe('Deploy at 10:00');
    });

    it('defaults source to manual', () => {
      const wf = engine.create('Test', 'wren:Do it');
      expect(wf.source).toBe('manual');
    });
  });

  // --- advance ---

  describe('advance', () => {
    it('completes current step and unlocks next', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      const result = engine.advance(wf.id, 'ADR written', 'architect/adr.md');

      expect(result.completedStep.seq).toBe(1);
      expect(result.completedStep.status).toBe('completed');
      expect(result.completedStep.notes).toBe('ADR written');
      expect(result.completedStep.artifacts).toEqual(['architect/adr.md']);
      expect(result.nextStep).not.toBeNull();
      expect(result.nextStep!.seq).toBe(2);
      expect(result.nextStep!.status).toBe('ready');
      expect(result.workflowCompleted).toBe(false);
    });

    it('completes workflow when last step advances', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      engine.advance(wf.id);
      const result = engine.advance(wf.id);

      expect(result.workflowCompleted).toBe(true);
      expect(result.nextStep).toBeNull();
      expect(result.manifest.status).toBe('completed');
    });

    it('archives completed workflow', () => {
      const wf = engine.create('Test', 'wren:Single step');
      engine.advance(wf.id);

      expect(existsSync(join(config.activeDir, 'WF-001.json'))).toBe(false);
      expect(existsSync(join(config.archiveDir, 'WF-001.json'))).toBe(true);
    });

    it('generates handoff brief for next step', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      const result = engine.advance(wf.id, 'Done designing');

      expect(result.briefPath).not.toBeNull();
      expect(existsSync(result.briefPath!)).toBe(true);
      const brief = readFileSync(result.briefPath!, 'utf-8');
      expect(brief).toContain('Workflow Handoff');
      expect(brief).toContain('kade');
      expect(brief).toContain('Done designing');
    });

    it('does not generate brief when workflow completes', () => {
      const wf = engine.create('Test', 'wren:Single step');
      const result = engine.advance(wf.id);
      expect(result.briefPath).toBeNull();
    });

    it('retires intermediate briefs when workflow completes', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build,wren:Verify');
      const r1 = engine.advance(wf.id, 'Step 1 done');
      const brief1 = r1.briefPath!;
      expect(existsSync(brief1)).toBe(true);

      const r2 = engine.advance(wf.id, 'Step 2 done');
      const brief2 = r2.briefPath!;
      expect(existsSync(brief2)).toBe(true);

      // Complete workflow
      engine.advance(wf.id, 'Step 3 done');

      // Both intermediate briefs should be renamed to .done
      expect(existsSync(brief1)).toBe(false);
      expect(existsSync(brief1 + '.done')).toBe(true);
      expect(existsSync(brief2)).toBe(false);
      expect(existsSync(brief2 + '.done')).toBe(true);
    });

    it('does not fail if brief was already removed', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      const r1 = engine.advance(wf.id, 'Step 1');
      // Manually delete the brief before workflow completes
      const { unlinkSync } = require('fs');
      unlinkSync(r1.briefPath!);

      // Should not throw
      expect(() => engine.advance(wf.id, 'Step 2')).not.toThrow();
    });

    it('records history events', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      engine.advance(wf.id);
      const loaded = engine.load(wf.id);

      expect(loaded.history.length).toBeGreaterThanOrEqual(3);
      const events = loaded.history.map(h => h.event);
      expect(events).toContain('created');
      expect(events).toContain('step_completed');
      expect(events).toContain('step_ready');
    });

    it('throws on already-completed workflow', () => {
      const wf = engine.create('Test', 'wren:Do it');
      engine.advance(wf.id);
      expect(() => engine.advance(wf.id)).toThrow('already completed');
    });

    it('throws on nonexistent workflow', () => {
      expect(() => engine.advance('WF-999')).toThrow('not found');
    });

    it('handles multiple artifacts', () => {
      const wf = engine.create('Test', 'wren:Do it');
      const result = engine.advance(wf.id, undefined, 'file1.md,file2.md,file3.md');
      expect(result.completedStep.artifacts).toEqual(['file1.md', 'file2.md', 'file3.md']);
    });

    it('sets completed_at timestamp', () => {
      const wf = engine.create('Test', 'wren:Do it');
      const result = engine.advance(wf.id);
      expect(result.completedStep.completed_at).toBeTruthy();
      expect(result.completedStep.completed_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    });

    it('logs handoff event to handoffs.log', () => {
      const wf = engine.create('Test', 'silas:Design,kade:Build');
      engine.advance(wf.id, 'ADR written', 'architect/adr.md');

      expect(existsSync(config.handoffLogPath)).toBe(true);
      const lines = readFileSync(config.handoffLogPath, 'utf-8').trim().split('\n');
      expect(lines).toHaveLength(1);
      const event = JSON.parse(lines[0]);
      expect(event.id).toBe('HO-WF-001-S1');
      expect(event.type).toBe('workflow-advance');
      expect(event.from).toBe('silas');
      expect(event.to).toBe('kade');
      expect(event.status).toBe('sent');
      expect(event.workflow).toBe('WF-001');
      expect(event.step).toBe(1);
    });

    it('does not log handoff when workflow completes', () => {
      const wf = engine.create('Test', 'wren:Single step');
      engine.advance(wf.id);

      expect(existsSync(config.handoffLogPath)).toBe(false);
    });

    it('logs multiple handoff events for multi-step workflow', () => {
      const wf = engine.create('Test', 'silas:A,kade:B,wren:C');
      engine.advance(wf.id, 'Step 1 done');
      engine.advance(wf.id, 'Step 2 done');

      const lines = readFileSync(config.handoffLogPath, 'utf-8').trim().split('\n');
      expect(lines).toHaveLength(2);
      const e1 = JSON.parse(lines[0]);
      const e2 = JSON.parse(lines[1]);
      expect(e1.from).toBe('silas');
      expect(e1.to).toBe('kade');
      expect(e2.from).toBe('kade');
      expect(e2.to).toBe('wren');
    });
  });

  // --- list ---

  describe('list', () => {
    it('returns empty array for no workflows', () => {
      expect(engine.list()).toEqual([]);
    });

    it('returns active workflows', () => {
      engine.create('First', 'wren:A');
      engine.create('Second', 'kade:B');
      expect(engine.list()).toHaveLength(2);
    });

    it('excludes archived by default', () => {
      const wf = engine.create('Done', 'wren:A');
      engine.advance(wf.id);
      engine.create('Active', 'kade:B');
      expect(engine.list()).toHaveLength(1);
      expect(engine.list()[0].decision).toBe('Active');
    });

    it('includes archived with all=true', () => {
      const wf = engine.create('Done', 'wren:A');
      engine.advance(wf.id);
      engine.create('Active', 'kade:B');
      expect(engine.list(true)).toHaveLength(2);
    });

    it('returns sorted by ID', () => {
      engine.create('B', 'wren:B');
      engine.create('A', 'kade:A');
      const list = engine.list();
      expect(list[0].id).toBe('WF-001');
      expect(list[1].id).toBe('WF-002');
    });
  });

  // --- pending ---

  describe('pending', () => {
    it('finds ready steps for a role', () => {
      engine.create('Test', 'silas:Design,kade:Build');
      const items = engine.pending('silas');
      expect(items).toHaveLength(1);
      expect(items[0].step.action).toBe('Design');
    });

    it('returns empty for role with no ready steps', () => {
      engine.create('Test', 'silas:Design,kade:Build');
      expect(engine.pending('kade')).toHaveLength(0);
    });

    it('finds steps across multiple workflows', () => {
      engine.create('First', 'wren:A');
      engine.create('Second', 'wren:B');
      expect(engine.pending('wren')).toHaveLength(2);
    });

    it('excludes completed workflows', () => {
      const wf = engine.create('Done', 'wren:A');
      engine.advance(wf.id);
      expect(engine.pending('wren')).toHaveLength(0);
    });

    it('throws on invalid role', () => {
      expect(() => engine.pending('bob')).toThrow('Invalid role');
    });
  });

  // --- history ---

  describe('history', () => {
    it('returns history events', () => {
      const wf = engine.create('Test', 'wren:A');
      const events = engine.history(wf.id);
      expect(events).toHaveLength(1);
      expect(events[0].event).toBe('created');
    });

    it('grows with advances', () => {
      const wf = engine.create('Test', 'silas:A,kade:B');
      engine.advance(wf.id);
      const events = engine.history(wf.id);
      expect(events.length).toBeGreaterThan(1);
    });

    it('throws on nonexistent workflow', () => {
      expect(() => engine.history('WF-999')).toThrow('not found');
    });
  });

  // --- load ---

  describe('load', () => {
    it('loads from active dir', () => {
      const wf = engine.create('Test', 'wren:A');
      const loaded = engine.load(wf.id);
      expect(loaded.decision).toBe('Test');
    });

    it('loads from archive dir', () => {
      const wf = engine.create('Test', 'wren:A');
      engine.advance(wf.id);
      const loaded = engine.load(wf.id);
      expect(loaded.status).toBe('completed');
    });

    it('throws on missing workflow', () => {
      expect(() => engine.load('WF-999')).toThrow('not found');
    });
  });

  // --- full lifecycle ---

  describe('full lifecycle', () => {
    it('handles a 3-step workflow end-to-end', () => {
      const wf = engine.create(
        'Migrate WordPress platform',
        'silas:Write ADR,kade:Implement migration,wren:Verify and update roadmap',
        'clearing:2026-02-22',
        118,
      );
      expect(wf.status).toBe('in_progress');
      expect(engine.pending('silas')).toHaveLength(1);
      expect(engine.pending('kade')).toHaveLength(0);

      // Step 1: Silas writes ADR
      const r1 = engine.advance(wf.id, 'ADR-015 written', 'architect/adr/ADR-015.md');
      expect(r1.workflowCompleted).toBe(false);
      expect(engine.pending('silas')).toHaveLength(0);
      expect(engine.pending('kade')).toHaveLength(1);

      // Step 2: Kade implements
      const r2 = engine.advance(wf.id, 'Migration complete');
      expect(r2.workflowCompleted).toBe(false);
      expect(engine.pending('wren')).toHaveLength(1);

      // Step 3: Wren verifies
      const r3 = engine.advance(wf.id, 'Verified and roadmap updated');
      expect(r3.workflowCompleted).toBe(true);
      expect(engine.list()).toHaveLength(0);
      expect(engine.list(true)).toHaveLength(1);

      // History should have all events
      const history = engine.history(wf.id);
      expect(history.length).toBeGreaterThanOrEqual(7);
      expect(history[history.length - 1].event).toBe('workflow_completed');
    });
  });

  describe('edge cases', () => {
    it('advance throws when no active step remains', () => {
      const cfg = tempConfig();
      const engine = new WorkflowEngine(cfg);
      const wf = engine.create('Stuck decision', 'silas:Write');
      // Force all steps to non-advanceable status directly on disk
      const manifestPath = join(cfg.activeDir, `${wf.id}.json`);
      const raw = JSON.parse(readFileSync(manifestPath, 'utf-8'));
      raw.steps[0].status = 'blocked';
      writeFileSync(manifestPath, JSON.stringify(raw));
      expect(() => engine.advance(wf.id)).toThrow(/No active step/);
      rmSync(cfg.activeDir, { recursive: true, force: true });
    });

  });
});
