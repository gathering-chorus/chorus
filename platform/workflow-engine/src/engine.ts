/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * fs paths in this workflow engine are all internally constructed:
 *   - Roots: this.config.activeDir / archiveDir (caller-supplied at construction)
 *   - Filenames: `WF-NNN.json` from this.nextId() or a wfId passed by callers
 *     who validate the id (CLI matches /^WF-\d+$/, server validates at handler boundary)
 *   - readdirSync results are filtered by /^WF-\d+\.json$/ regex before use
 * Object indexing is on internally-derived role/status keys from typed enums.
 */
import { readFileSync, writeFileSync, appendFileSync, readdirSync, renameSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import {
  WorkflowManifest, Step, HistoryEvent, AdvanceResult, PendingStep,
  WorkflowEngineConfig, HandoffEvent, Role, StepStatus,
} from './types';
import { DEFAULT_CONFIG, VALID_ROLES, isValidRole, nowISO } from './config';
import { generateHandoffBrief } from './brief';

export class WorkflowEngine {
  private config: WorkflowEngineConfig;

  constructor(config?: Partial<WorkflowEngineConfig>) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    mkdirSync(this.config.activeDir, { recursive: true });
    mkdirSync(this.config.archiveDir, { recursive: true });
  }

  // --- ID generation ---

  nextId(): string {
    const ids: number[] = [];
    for (const dir of [this.config.activeDir, this.config.archiveDir]) {
      if (!existsSync(dir)) continue;
      for (const f of readdirSync(dir)) {
        const m = f.match(/^WF-(\d+)\.json$/);
        if (m) ids.push(parseInt(m[1], 10));
      }
    }
    const next = ids.length > 0 ? Math.max(...ids) + 1 : 1;
    return `WF-${String(next).padStart(3, '0')}`;
  }

  // --- File operations ---

  private manifestPath(wfId: string): string {
    return join(this.config.activeDir, `${wfId}.json`);
  }

  private archivePath(wfId: string): string {
    return join(this.config.archiveDir, `${wfId}.json`);
  }

  load(wfId: string): WorkflowManifest {
    const activePath = this.manifestPath(wfId);
    if (existsSync(activePath)) {
      return JSON.parse(readFileSync(activePath, 'utf-8'));
    }
    const archPath = this.archivePath(wfId);
    if (existsSync(archPath)) {
      return JSON.parse(readFileSync(archPath, 'utf-8'));
    }
    throw new Error(`Workflow ${wfId} not found`);
  }

  private save(manifest: WorkflowManifest): void {
    const p = this.manifestPath(manifest.id);
    writeFileSync(p, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
  }

  private archive(manifest: WorkflowManifest): void {
    const src = this.manifestPath(manifest.id);
    const dst = this.archivePath(manifest.id);
    if (existsSync(src)) {
      renameSync(src, dst);
    } else {
      writeFileSync(dst, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
    }
  }

  // --- Scan ---

  scanWorkflows(includeArchive = false): WorkflowManifest[] {
    const results: WorkflowManifest[] = [];
    const dirs = [this.config.activeDir];
    if (includeArchive) dirs.push(this.config.archiveDir);

    for (const dir of dirs) {
      if (!existsSync(dir)) continue;
      for (const f of readdirSync(dir)) {
        if (!f.match(/^WF-\d+\.json$/)) continue;
        try {
          results.push(JSON.parse(readFileSync(join(dir, f), 'utf-8')));
        } catch {
          // Skip malformed files
        }
      }
    }

    return results.sort((a, b) => a.id.localeCompare(b.id));
  }

  // --- Commands ---

  create(decision: string, stepsStr: string, source = 'manual', card?: number): WorkflowManifest {
    if (!decision) throw new Error('Decision text is required');
    if (!stepsStr) throw new Error('Steps are required (format: role:action,role:action)');

    const parsedSteps = stepsStr.split(',').map((s, i) => {
      const parts = s.trim().split(':');
      if (parts.length < 2) throw new Error(`Invalid step format: "${s.trim()}". Use role:action`);
      const role = parts[0].trim();
      const action = parts.slice(1).join(':').trim();
      if (!isValidRole(role)) throw new Error(`Invalid role: "${role}". Valid: ${VALID_ROLES.join(', ')}`);
      return { role: role as Role, action, seq: i + 1 };
    });

    if (parsedSteps.length === 0) throw new Error('At least one step is required');

    const now = nowISO();
    const id = this.nextId();

    const steps: Step[] = parsedSteps.map((p, i) => ({
      seq: p.seq,
      role: p.role,
      action: p.action,
      status: (i === 0 ? 'ready' : 'pending') as StepStatus,
      card: null,
      blocked_by: i === 0 ? [] : [i], // Each step blocked by previous
      artifacts: [],
      brief: null,
      started_at: null,
      completed_at: null,
      notes: null,
    }));

    const manifest: WorkflowManifest = {
      id,
      decision,
      source,
      card: card ?? null,
      created: now,
      updated: now,
      status: 'in_progress',
      steps,
      verification: null,
      history: [{
        timestamp: now,
        event: 'created',
        role: 'system',
        detail: `Workflow created: ${decision}`,
      }],
    };

    this.save(manifest);
    return manifest;
  }

  private completeStep(step: Step, now: string, notes: string | undefined, artifacts: string | undefined, manifest: WorkflowManifest): void {
    step.status = 'completed';
    step.completed_at = now;
    if (notes) step.notes = notes;
    if (artifacts) step.artifacts = artifacts.split(',').map((a) => a.trim()).filter(Boolean);
    manifest.history.push({
      timestamp: now,
      event: 'step_completed',
      role: step.role,
      detail: `Step ${step.seq} completed: ${step.role} — ${step.action}`,
    });
  }

  private unlockNextStep(manifest: WorkflowManifest, now: string): Step | null {
    for (const step of manifest.steps) {
      if (step.status !== 'pending') continue;
      const allBlockersComplete = step.blocked_by.every((blockerSeq) => {
        const blocker = manifest.steps.find((s) => s.seq === blockerSeq);
        return blocker && (blocker.status === 'completed' || blocker.status === 'skipped');
      });
      if (!allBlockersComplete) continue;
      step.status = 'ready';
      manifest.history.push({
        timestamp: now,
        event: 'step_ready',
        role: step.role,
        detail: `Step ${step.seq} ready: ${step.role} — ${step.action}`,
      });
      return step;
    }
    return null;
  }

  private writeHandoffBrief(manifest: WorkflowManifest, currentStep: Step, nextStep: Step, now: string): string | null {
    const briefDir = this.config.briefDirs[nextStep.role];
    if (!briefDir) return null;
    mkdirSync(briefDir, { recursive: true });
    const fileName = `${now.split('T')[0]}-${manifest.id.toLowerCase()}-step${nextStep.seq}.md`;
    const briefPath = join(briefDir, fileName);
    writeFileSync(briefPath, generateHandoffBrief(manifest, currentStep, nextStep), 'utf-8');
    nextStep.brief = briefPath;
    return briefPath;
  }

  advance(wfId: string, notes?: string, artifacts?: string): AdvanceResult {
    const manifest = this.load(wfId);
    if (manifest.status === 'completed') throw new Error(`Workflow ${wfId} is already completed`);

    const currentStep = manifest.steps.find((s) => s.status === 'in_progress' || s.status === 'ready');
    if (!currentStep) throw new Error(`No active step to advance in ${wfId}`);

    const now = nowISO();
    this.completeStep(currentStep, now, notes, artifacts, manifest);
    const nextStep = this.unlockNextStep(manifest, now);

    const allDone = manifest.steps.every((s) => s.status === 'completed' || s.status === 'skipped');
    if (allDone) {
      manifest.status = 'completed';
      manifest.history.push({
        timestamp: now, event: 'workflow_completed', role: 'system',
        detail: `Workflow completed: ${manifest.decision}`,
      });
    }
    manifest.updated = now;

    const briefPath = nextStep && !allDone ? this.writeHandoffBrief(manifest, currentStep, nextStep, now) : null;
    if (nextStep && briefPath) {
      this.logHandoff({
        id: `HO-${manifest.id}-S${currentStep.seq}`,
        type: 'workflow-advance',
        from: currentStep.role, to: nextStep.role,
        artifact: briefPath, status: 'sent', timestamp: now,
        workflow: manifest.id, step: currentStep.seq,
      });
    }

    this.save(manifest);
    if (allDone) {
      this.archive(manifest);
      this.retireBriefs(manifest);
    }

    return { completedStep: currentStep, nextStep, workflowCompleted: allDone, briefPath, manifest };
  }

  private retireBriefs(manifest: WorkflowManifest): void {
    for (const step of manifest.steps) {
      if (step.brief && existsSync(step.brief)) {
        try {
          renameSync(step.brief, step.brief + '.done');
        } catch {
          // Brief may have been moved or deleted — not critical
        }
      }
    }
  }

  private logHandoff(event: HandoffEvent): void {
    try {
      mkdirSync(dirname(this.config.handoffLogPath), { recursive: true });
      appendFileSync(this.config.handoffLogPath, JSON.stringify(event) + '\n', 'utf-8');
    } catch {
      // Log dir may not exist in some configs — fail silently
    }
  }

  status(wfId?: string): WorkflowManifest | WorkflowManifest[] {
    if (wfId) return this.load(wfId);
    return this.scanWorkflows();
  }

  list(all = false): WorkflowManifest[] {
    return this.scanWorkflows(all);
  }

  pending(role: string): PendingStep[] {
    if (!isValidRole(role)) throw new Error(`Invalid role: "${role}"`);
    const results: PendingStep[] = [];
    const workflows = this.scanWorkflows();
    for (const wf of workflows) {
      if (wf.status === 'completed' || wf.status === 'cancelled') continue;
      for (const step of wf.steps) {
        if (step.role === role && step.status === 'ready') {
          results.push({
            workflowId: wf.id,
            decision: wf.decision,
            step,
            card: wf.card,
          });
        }
      }
    }
    return results;
  }

  history(wfId: string): HistoryEvent[] {
    const manifest = this.load(wfId);
    return manifest.history;
  }
}
