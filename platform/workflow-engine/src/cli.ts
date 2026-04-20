#!/usr/bin/env node

import { WorkflowEngine } from './engine';
import { WorkflowManifest, Step } from './types';
import { appendFileSync } from 'fs';
import { resolve } from 'path';

const LOG_FILE = resolve(__dirname, '../../logs/chorus.log');

function die(msg: string): never {
  process.stderr.write(`Error: ${msg}\n`);
  process.exit(1);
}

function log(event: string, extras: Record<string, string> = {}): void {
  const entry = {
    timestamp: new Date().toISOString(),
    level: 'info',
    appName: 'workflow-engine',
    component: 'cli',
    event,
    ...extras,
  };
  try {
    appendFileSync(LOG_FILE, JSON.stringify(entry) + '\n');
  } catch {
    // Silent — don't break workflow for logging failure
  }
}

function formatStep(s: Step): string {
  const icons: Record<string, string> = {
    completed: '\u2705',
    ready: '\u27A1\uFE0F ',
    in_progress: '\u{1F504}',
    pending: '\u23F3',
    blocked: '\u{1F6D1}',
    skipped: '\u23ED\uFE0F ',
  };
  const icon = icons[s.status] || '?';
  return `  ${icon} Step ${s.seq}: ${s.role} — ${s.action}`;
}

function formatWorkflow(wf: WorkflowManifest): string {
  const total = wf.steps.length;
  const done = wf.steps.filter(s => s.status === 'completed' || s.status === 'skipped').length;
  const bar = `[${done}/${total}]`;
  const statusIcon = wf.status === 'completed' ? '\u2705' : '\u{1F504}';
  return `${statusIcon} ${wf.id} ${bar} — ${wf.decision}`;
}

function usage(): void {
  console.log(`workflow-ts — Workflow execution engine

Commands:
  create "decision" --steps role:action,role:action [--source src] [--card N]
  advance WF-NNN [--notes "text"] [--artifacts file1,file2]
  status [WF-NNN]
  list [--all]
  pending <role>
  history WF-NNN

Step format: role:action — roles are silas, kade, wren, jeff
Steps execute in order. Each step blocks on the previous.

Examples:
  workflow-ts create "Migrate WordPress" \\
    --steps "silas:Write ADR,kade:Implement,wren:Verify" \\
    --source clearing:2026-02-22 --card 118

  workflow-ts advance WF-001 --notes "ADR written" --artifacts architect/adr/ADR-015.md
  workflow-ts pending silas`);
}

function parseArgs(argv: string[]): { command: string; args: string[]; flags: Record<string, string> } {
  const command = argv[0] || 'list';
  const args: string[] = [];
  const flags: Record<string, string> = {};

  for (let i = 1; i < argv.length; i++) {
    if (argv[i].startsWith('--')) {
      const key = argv[i].slice(2);
      if (key === 'all' || key === 'open') {
        flags[key] = 'true';
      } else if (i + 1 < argv.length) {
        flags[key] = argv[++i];
      }
    } else {
      args.push(argv[i]);
    }
  }

  return { command, args, flags };
}

function main(): void {
  const rawArgs = process.argv.slice(2);

  if (rawArgs.length === 0 || rawArgs[0] === '--help' || rawArgs[0] === '-h') {
    usage();
    return;
  }

  const { command, args, flags } = parseArgs(rawArgs);
  const engine = new WorkflowEngine();

  switch (command) {
    case 'create': {
      const decision = args[0];
      if (!decision) die('Usage: workflow-ts create "decision" --steps role:action,...');
      const steps = flags['steps'];
      if (!steps) die('--steps is required');
      const card = flags['card'] ? parseInt(flags['card'], 10) : undefined;
      const wf = engine.create(decision, steps, flags['source'] || 'manual', card);
      log('workflow.manifest.created', { id: wf.id, decision: wf.decision });
      console.log(`Created: ${wf.id}`);
      wf.steps.forEach(s => console.log(formatStep(s)));
      break;
    }

    case 'advance': {
      const wfId = args[0];
      if (!wfId) die('Usage: workflow-ts advance WF-NNN [--notes "..."] [--artifacts "..."]');
      const result = engine.advance(wfId, flags['notes'], flags['artifacts']);
      log('workflow.step.completed', {
        id: result.manifest.id,
        step: String(result.completedStep.seq),
        role: result.completedStep.role,
      });
      console.log(`Step ${result.completedStep.seq} completed (${result.completedStep.role}: ${result.completedStep.action})`);
      if (result.workflowCompleted) {
        console.log('Workflow complete — all steps done');
      } else if (result.nextStep) {
        console.log(`Step ${result.nextStep.seq} now READY for ${result.nextStep.role}: ${result.nextStep.action}`);
      }
      if (result.briefPath) {
        console.log(`Handoff brief sent: ${result.briefPath}`);
      }
      break;
    }

    case 'status': {
      const wfId = args[0];
      if (wfId) {
        const wf = engine.status(wfId) as WorkflowManifest;
        console.log(formatWorkflow(wf));
        wf.steps.forEach(s => console.log(formatStep(s)));
        if (wf.card) console.log(`\n  Card: #${wf.card}`);
        console.log(`  Source: ${wf.source}`);
        console.log(`  Created: ${wf.created}`);
        console.log(`  Updated: ${wf.updated}`);
      } else {
        const workflows = engine.status() as WorkflowManifest[];
        if (workflows.length === 0) {
          console.log('No active workflows');
        } else {
          workflows.forEach(wf => {
            console.log(formatWorkflow(wf));
            wf.steps.forEach(s => console.log(formatStep(s)));
            console.log();
          });
        }
      }
      break;
    }

    case 'list': {
      const all = flags['all'] === 'true';
      const workflows = engine.list(all);
      if (workflows.length === 0) {
        console.log(all ? 'No workflows' : 'No active workflows');
      } else {
        workflows.forEach(wf => console.log(formatWorkflow(wf)));
      }
      break;
    }

    case 'pending': {
      const role = args[0];
      if (!role) die('Usage: workflow-ts pending <role>');
      const items = engine.pending(role);
      if (items.length === 0) {
        console.log(`No pending steps for ${role}`);
      } else {
        items.forEach(item => {
          console.log(`${item.workflowId}: ${item.step.action}`);
          console.log(`  Decision: ${item.decision}`);
          if (item.card) console.log(`  Card: #${item.card}`);
        });
      }
      break;
    }

    case 'history': {
      const wfId = args[0];
      if (!wfId) die('Usage: workflow-ts history WF-NNN');
      const events = engine.history(wfId);
      events.forEach(e => {
        console.log(`${e.timestamp} | ${e.role} | ${e.event} | ${e.detail}`);
      });
      break;
    }

    default:
      die(`Unknown command: ${command}. Run workflow-ts --help`);
  }
}

main();
