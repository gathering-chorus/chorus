"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.WorkflowEngine = void 0;
const fs_1 = require("fs");
const path_1 = require("path");
const config_1 = require("./config");
const brief_1 = require("./brief");
class WorkflowEngine {
    config;
    constructor(config) {
        this.config = { ...config_1.DEFAULT_CONFIG, ...config };
        (0, fs_1.mkdirSync)(this.config.activeDir, { recursive: true });
        (0, fs_1.mkdirSync)(this.config.archiveDir, { recursive: true });
    }
    // --- ID generation ---
    nextId() {
        const ids = [];
        for (const dir of [this.config.activeDir, this.config.archiveDir]) {
            if (!(0, fs_1.existsSync)(dir))
                continue;
            for (const f of (0, fs_1.readdirSync)(dir)) {
                const m = f.match(/^WF-(\d+)\.json$/);
                if (m)
                    ids.push(parseInt(m[1], 10));
            }
        }
        const next = ids.length > 0 ? Math.max(...ids) + 1 : 1;
        return `WF-${String(next).padStart(3, '0')}`;
    }
    // --- File operations ---
    manifestPath(wfId) {
        return (0, path_1.join)(this.config.activeDir, `${wfId}.json`);
    }
    archivePath(wfId) {
        return (0, path_1.join)(this.config.archiveDir, `${wfId}.json`);
    }
    load(wfId) {
        const activePath = this.manifestPath(wfId);
        if ((0, fs_1.existsSync)(activePath)) {
            return JSON.parse((0, fs_1.readFileSync)(activePath, 'utf-8'));
        }
        const archPath = this.archivePath(wfId);
        if ((0, fs_1.existsSync)(archPath)) {
            return JSON.parse((0, fs_1.readFileSync)(archPath, 'utf-8'));
        }
        throw new Error(`Workflow ${wfId} not found`);
    }
    save(manifest) {
        const p = this.manifestPath(manifest.id);
        (0, fs_1.writeFileSync)(p, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
    }
    archive(manifest) {
        const src = this.manifestPath(manifest.id);
        const dst = this.archivePath(manifest.id);
        if ((0, fs_1.existsSync)(src)) {
            (0, fs_1.renameSync)(src, dst);
        }
        else {
            (0, fs_1.writeFileSync)(dst, JSON.stringify(manifest, null, 2) + '\n', 'utf-8');
        }
    }
    // --- Scan ---
    scanWorkflows(includeArchive = false) {
        const results = [];
        const dirs = [this.config.activeDir];
        if (includeArchive)
            dirs.push(this.config.archiveDir);
        for (const dir of dirs) {
            if (!(0, fs_1.existsSync)(dir))
                continue;
            for (const f of (0, fs_1.readdirSync)(dir)) {
                if (!f.match(/^WF-\d+\.json$/))
                    continue;
                try {
                    results.push(JSON.parse((0, fs_1.readFileSync)((0, path_1.join)(dir, f), 'utf-8')));
                }
                catch {
                    // Skip malformed files
                }
            }
        }
        return results.sort((a, b) => a.id.localeCompare(b.id));
    }
    // --- Commands ---
    create(decision, stepsStr, source = 'manual', card) {
        if (!decision)
            throw new Error('Decision text is required');
        if (!stepsStr)
            throw new Error('Steps are required (format: role:action,role:action)');
        const parsedSteps = stepsStr.split(',').map((s, i) => {
            const parts = s.trim().split(':');
            if (parts.length < 2)
                throw new Error(`Invalid step format: "${s.trim()}". Use role:action`);
            const role = parts[0].trim();
            const action = parts.slice(1).join(':').trim();
            if (!(0, config_1.isValidRole)(role))
                throw new Error(`Invalid role: "${role}". Valid: ${config_1.VALID_ROLES.join(', ')}`);
            return { role: role, action, seq: i + 1 };
        });
        if (parsedSteps.length === 0)
            throw new Error('At least one step is required');
        const now = (0, config_1.nowISO)();
        const id = this.nextId();
        const steps = parsedSteps.map((p, i) => ({
            seq: p.seq,
            role: p.role,
            action: p.action,
            status: (i === 0 ? 'ready' : 'pending'),
            card: null,
            blocked_by: i === 0 ? [] : [i], // Each step blocked by previous
            artifacts: [],
            brief: null,
            started_at: null,
            completed_at: null,
            notes: null,
        }));
        const manifest = {
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
    advance(wfId, notes, artifacts) {
        const manifest = this.load(wfId);
        if (manifest.status === 'completed') {
            throw new Error(`Workflow ${wfId} is already completed`);
        }
        // Find current step: first in_progress or ready
        const currentStep = manifest.steps.find(s => s.status === 'in_progress' || s.status === 'ready');
        if (!currentStep) {
            throw new Error(`No active step to advance in ${wfId}`);
        }
        const now = (0, config_1.nowISO)();
        // Complete current step
        currentStep.status = 'completed';
        currentStep.completed_at = now;
        if (notes)
            currentStep.notes = notes;
        if (artifacts) {
            currentStep.artifacts = artifacts.split(',').map(a => a.trim()).filter(Boolean);
        }
        manifest.history.push({
            timestamp: now,
            event: 'step_completed',
            role: currentStep.role,
            detail: `Step ${currentStep.seq} completed: ${currentStep.role} — ${currentStep.action}`,
        });
        // Find and unlock next step
        let nextStep = null;
        for (const step of manifest.steps) {
            if (step.status !== 'pending')
                continue;
            const allBlockersComplete = step.blocked_by.every(blockerSeq => {
                const blocker = manifest.steps.find(s => s.seq === blockerSeq);
                return blocker && (blocker.status === 'completed' || blocker.status === 'skipped');
            });
            if (allBlockersComplete) {
                step.status = 'ready';
                nextStep = step;
                manifest.history.push({
                    timestamp: now,
                    event: 'step_ready',
                    role: step.role,
                    detail: `Step ${step.seq} ready: ${step.role} — ${step.action}`,
                });
                break; // Only unlock one at a time (sequential model)
            }
        }
        // Check if workflow is complete
        const allDone = manifest.steps.every(s => s.status === 'completed' || s.status === 'skipped');
        if (allDone) {
            manifest.status = 'completed';
            manifest.history.push({
                timestamp: now,
                event: 'workflow_completed',
                role: 'system',
                detail: `Workflow completed: ${manifest.decision}`,
            });
        }
        manifest.updated = now;
        // Generate handoff brief if next step exists
        let briefPath = null;
        if (nextStep && !allDone) {
            const briefDir = this.config.briefDirs[nextStep.role];
            if (briefDir) {
                (0, fs_1.mkdirSync)(briefDir, { recursive: true });
                const dateStr = now.split('T')[0];
                const wfNum = manifest.id.toLowerCase();
                const fileName = `${dateStr}-${wfNum}-step${nextStep.seq}.md`;
                briefPath = (0, path_1.join)(briefDir, fileName);
                const briefContent = (0, brief_1.generateHandoffBrief)(manifest, currentStep, nextStep);
                (0, fs_1.writeFileSync)(briefPath, briefContent, 'utf-8');
                nextStep.brief = briefPath;
            }
        }
        // Log handoff event if handing off to next role
        if (nextStep && briefPath) {
            this.logHandoff({
                id: `HO-${manifest.id}-S${currentStep.seq}`,
                type: 'workflow-advance',
                from: currentStep.role,
                to: nextStep.role,
                artifact: briefPath,
                status: 'sent',
                timestamp: now,
                workflow: manifest.id,
                step: currentStep.seq,
            });
        }
        // Save updated manifest, then archive if done
        this.save(manifest);
        if (allDone) {
            this.archive(manifest);
            this.retireBriefs(manifest);
        }
        return {
            completedStep: currentStep,
            nextStep,
            workflowCompleted: allDone,
            briefPath,
            manifest,
        };
    }
    retireBriefs(manifest) {
        for (const step of manifest.steps) {
            if (step.brief && (0, fs_1.existsSync)(step.brief)) {
                try {
                    (0, fs_1.renameSync)(step.brief, step.brief + '.done');
                }
                catch {
                    // Brief may have been moved or deleted — not critical
                }
            }
        }
    }
    logHandoff(event) {
        try {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(this.config.handoffLogPath), { recursive: true });
            (0, fs_1.appendFileSync)(this.config.handoffLogPath, JSON.stringify(event) + '\n', 'utf-8');
        }
        catch {
            // Log dir may not exist in some configs — fail silently
        }
    }
    status(wfId) {
        if (wfId)
            return this.load(wfId);
        return this.scanWorkflows();
    }
    list(all = false) {
        return this.scanWorkflows(all);
    }
    pending(role) {
        if (!(0, config_1.isValidRole)(role))
            throw new Error(`Invalid role: "${role}"`);
        const results = [];
        const workflows = this.scanWorkflows();
        for (const wf of workflows) {
            if (wf.status === 'completed' || wf.status === 'cancelled')
                continue;
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
    history(wfId) {
        const manifest = this.load(wfId);
        return manifest.history;
    }
}
exports.WorkflowEngine = WorkflowEngine;
//# sourceMappingURL=engine.js.map