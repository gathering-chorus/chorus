/**
 * Hooks summary — #2099 per-page migration from Gathering.
 *
 * Reads chorus logs, classifies governance events, and returns summaries +
 * totals grouped by hook category. Consumed by /api/chorus/hooks/summary
 * and the static /borg/hooks/ page. Classifier logic mirrors the original
 * handler in jeff-bridwell-personal-site/src/handlers/hooks.handler.ts —
 * same category keys, same event → category mapping — so the dashboard
 * shows identical counts as Jeff's used in Gathering.
 */

import * as fs from 'fs';

const CHORUS_ROOT = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects/chorus';

const LOG_PATHS = {
  chorus: process.env.CHORUS_LOG_PATH || `${CHORUS_ROOT}/platform/logs/chorus.log`,
  permissions: process.env.PERMISSIONS_LOG_PATH || `${CHORUS_ROOT}/platform/logs/permission-prompts.log`,
  errors: process.env.COMMAND_ERRORS_LOG_PATH || `${CHORUS_ROOT}/platform/logs/command-errors.log`,
  handoffs: process.env.HANDOFFS_LOG_PATH || `${CHORUS_ROOT}/proving/logs/handoffs.log`,
};

type HookCategory =
  | 'search-hierarchy' | 'decision-gate' | 'jdi-gate' | 'app-state-guard'
  | 'sparql-guard' | 'permission-logger' | 'nudge' | 'build-gate'
  | 'card-quality' | 'deploy-gate' | 'sensitive-paths' | 'credential-guard'
  | 'ops-health' | 'other';

type EnforcementTier = 'enforced' | 'advisory';

interface HookEvent {
  timestamp: string;
  category: HookCategory;
  action: 'block' | 'flag' | 'nudge' | 'allow' | 'log';
  role: string;
  detail: string;
  raw: Record<string, unknown>;
}

export interface HookSummary {
  category: HookCategory;
  label: string;
  description: string;
  enforcement: EnforcementTier;
  today: number;
  last7d: number;
  blocks: number;
  flags: number;
  nudges: number;
  recent: HookEvent[];
}

export interface HooksSummaryResponse {
  summaries: HookSummary[];
  totals: {
    today: number;
    last7d: number;
    blocks: number;
    flags: number;
    nudges: number;
  };
}

const CATEGORIES: Array<{ key: HookCategory; label: string; description: string; enforcement: EnforcementTier }> = [
  { key: 'search-hierarchy', label: 'Search Hierarchy', description: 'Filesystem searches that should have started with Chorus or codebase graph', enforcement: 'advisory' },
  { key: 'decision-gate', label: 'Decision Gate', description: 'Questions matching known Jeff preferences — blocked or nudged', enforcement: 'advisory' },
  { key: 'jdi-gate', label: 'JDI Gate', description: 'Permission-seeking responses blocked — role should execute, not ask', enforcement: 'enforced' },
  { key: 'app-state-guard', label: 'App State Guard', description: 'Direct docker/kill commands blocked — must use app-state.sh', enforcement: 'enforced' },
  { key: 'sparql-guard', label: 'SPARQL Guard', description: 'Bare triple pattern queries warned — use named graphs', enforcement: 'advisory' },
  { key: 'card-quality', label: 'Card Quality', description: 'Card gate violations — missing AC, blast radius failures, quality warnings', enforcement: 'enforced' },
  { key: 'deploy-gate', label: 'Deploy Gate', description: 'Deploy pipeline events — skipped deploys, freeze checks', enforcement: 'advisory' },
  { key: 'sensitive-paths', label: 'Sensitive Paths', description: 'File classification decisions — deny, ask, or allow on guarded paths', enforcement: 'enforced' },
  { key: 'credential-guard', label: 'Credential Guard', description: 'Write scrubber blocks — credentials detected in file writes', enforcement: 'enforced' },
  { key: 'ops-health', label: 'Ops Health', description: 'Operational alerts — fired and resolved health events', enforcement: 'advisory' },
  { key: 'nudge', label: 'Role Nudge', description: 'Cross-role nudges sent and delivered', enforcement: 'advisory' },
  { key: 'build-gate', label: 'Build Gate', description: 'Pre-commit, pre-push checks — lint, test, security', enforcement: 'enforced' },
  { key: 'permission-logger', label: 'Permission Logger', description: 'Tool calls logged for audit trail', enforcement: 'advisory' },
];

const CLASSIFIERS: Array<{
  match: (event: string) => boolean;
  classify: (obj: Record<string, unknown>, event: string) => Pick<HookEvent, 'category' | 'action' | 'detail'> | null;
}> = [
  { match: (e) => e === 'search.hierarchy.filesystem_used',
    classify: (obj) => ({ category: 'search-hierarchy', action: 'flag', detail: `${obj.tool || '?'}: ${obj.pattern || ''}` }) },
  { match: (e) => e === 'decision.gate.matched',
    classify: (obj) => ({ category: 'decision-gate', action: 'flag', detail: `Pref ${obj.pref || '?'}: ${String(obj.question || '').slice(0, 120)}` }) },
  { match: (e) => e === 'decision.gate.pass',
    classify: (obj) => ({ category: 'decision-gate', action: 'allow', detail: `Passed: ${String(obj.question || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'decision.gate.text_leak',
    classify: (obj) => ({ category: 'jdi-gate', action: 'block', detail: `Response blocked: permission-seeking detected (${obj.hook || 'jdi-gate'})` }) },
  { match: (e) => e === 'decision.gate.jdi_override',
    classify: (obj) => ({ category: 'jdi-gate', action: 'allow', detail: `Jeff override: ${String(obj.detail || 'manual jdi').slice(0, 120)}` }) },
  { match: (e) => e === 'guard.rule.decided',
    classify: (obj) => {
      const pattern = String(obj.pattern || '');
      const decision = String(obj.decision || '');
      return { category: 'app-state-guard', action: decision === 'deny' ? 'block' : 'allow', detail: `${decision}: ${pattern} — ${String(obj.command || '').slice(0, 100)}` };
    } },
  { match: (e) => e === 'guard.sparql.warned',
    classify: (obj) => ({ category: 'sparql-guard', action: 'flag', detail: String(obj.detail || obj.query || '').slice(0, 120) }) },
  { match: (e) => e.startsWith('role.nudge.') || e.startsWith('nudge.'),
    classify: (obj, event) => ({ category: 'nudge', action: event.includes('delivered') || event.includes('sent') ? 'nudge' : 'log', detail: `${event.split('.').pop()}: ${String(obj.detail || obj.to || '')}`.slice(0, 120) }) },
  { match: (e) => e === 'build.precommit.completed' || e === 'build.commit.created',
    classify: (obj, event) => ({ category: 'build-gate', action: 'log', detail: `${event}: checks=${obj.checks_passed || 0}/${obj.checks_run || 0}` }) },
  { match: (e) => e === 'card.quality.blocked',
    classify: (obj) => ({ category: 'card-quality', action: 'block', detail: `Blocked: ${String(obj.reason || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'card.quality.warned',
    classify: (obj) => ({ category: 'card-quality', action: 'flag', detail: `Warning: ${String(obj.reason || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'card.blast_radius.failed',
    classify: (obj) => ({ category: 'card-quality', action: 'block', detail: `Blast radius failed: ${String(obj.card || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'build.queue.blocked',
    classify: (obj) => ({ category: 'build-gate', action: 'block', detail: `Queue blocked: ${String(obj.reason || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'build.prepush.timed' || e === 'build.prepush.started' || e === 'build.push.completed' || e === 'build.tsc.completed',
    classify: (obj, event) => ({ category: 'build-gate', action: 'log', detail: `${event}: ${String(obj.duration || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'deploy.pipeline.skipped' || e === 'deploy.skipped',
    classify: (obj, event) => ({ category: 'deploy-gate', action: 'flag', detail: `${event}: ${String(obj.reason || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'guard.classify.decided',
    classify: (obj) => {
      const decision = String(obj.decision || '');
      const action: HookEvent['action'] = decision === 'deny' ? 'block' : decision === 'ask' ? 'flag' : 'allow';
      return { category: 'sensitive-paths', action, detail: `${decision}: ${String(obj.path || obj.detail || '').slice(0, 120)}` };
    } },
  { match: (e) => e === 'guard.scrub.blocked',
    classify: (obj) => {
      const decision = String(obj.decision || obj.action || '');
      return { category: 'credential-guard', action: decision === 'warn' ? 'flag' : 'block', detail: `Scrub ${decision}: ${String(obj.path || obj.detail || '').slice(0, 120)}` };
    } },
  { match: (e) => e === 'ops.alert.fired',
    classify: (obj) => ({ category: 'ops-health', action: 'flag', detail: `Alert: ${String(obj.alert || obj.detail || '').slice(0, 120)}` }) },
  { match: (e) => e === 'ops.alert.resolved',
    classify: (obj) => ({ category: 'ops-health', action: 'allow', detail: `Resolved: ${String(obj.alert || obj.detail || '').slice(0, 120)}` }) },
];

function readLogLines(filePath: string): string[] {
  try {
    if (!fs.existsSync(filePath)) return [];
    return fs.readFileSync(filePath, 'utf-8').trim().split('\n').filter(Boolean);
  } catch {
    return [];
  }
}

function classifyChorusEvent(obj: Record<string, unknown>): HookEvent | null {
  const event = String(obj.event || '');
  const ts = String(obj.timestamp || '');
  const role = String(obj.role || 'unknown');
  for (const { match, classify } of CLASSIFIERS) {
    if (!match(event)) continue;
    const result = classify(obj, event);
    if (!result) return null;
    return { timestamp: ts, role, raw: obj, ...result };
  }
  return null;
}

function loadAllEvents(): HookEvent[] {
  const events: HookEvent[] = [];

  for (const line of readLogLines(LOG_PATHS.chorus)) {
    try {
      const obj = JSON.parse(line);
      const ev = classifyChorusEvent(obj);
      if (ev) events.push(ev);
    } catch { /* skip malformed */ }
  }

  for (const line of readLogLines(LOG_PATHS.permissions)) {
    try {
      const obj = JSON.parse(line);
      events.push({
        timestamp: obj.timestamp || '',
        category: 'permission-logger',
        action: 'log',
        role: obj.role || 'unknown',
        detail: `${obj.tool || '?'}: ${(obj.detail || '').slice(0, 120)}`,
        raw: obj,
      });
    } catch { /* skip */ }
  }

  for (const line of readLogLines(LOG_PATHS.errors)) {
    try {
      const obj = JSON.parse(line);
      if (obj.fingerprint === 'DOCKER_BLOCKED' || obj.fingerprint === 'KILL_BLOCKED') {
        events.push({
          timestamp: obj.ts || '',
          category: 'app-state-guard',
          action: 'block',
          role: obj.role || 'unknown',
          detail: `Blocked: ${(obj.cmd || '').slice(0, 120)}`,
          raw: obj,
        });
      }
    } catch { /* skip */ }
  }

  events.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
  return events;
}

export function getHooksSummary(): HooksSummaryResponse {
  const events = loadAllEvents();
  const now = new Date();
  const todayStr = now.toISOString().slice(0, 10);
  const weekAgo = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000);

  const summaries: HookSummary[] = CATEGORIES.map((cat) => {
    const catEvents = events.filter((e) => e.category === cat.key);
    const todayEvents = catEvents.filter((e) => e.timestamp.startsWith(todayStr));
    const weekEvents = catEvents.filter((e) => new Date(e.timestamp) >= weekAgo);
    return {
      category: cat.key,
      label: cat.label,
      description: cat.description,
      enforcement: cat.enforcement,
      today: todayEvents.length,
      last7d: weekEvents.length,
      blocks: weekEvents.filter((e) => e.action === 'block').length,
      flags: weekEvents.filter((e) => e.action === 'flag').length,
      nudges: weekEvents.filter((e) => e.action === 'nudge').length,
      recent: catEvents.slice(-5).reverse(),
    };
  });

  const totals = {
    today: summaries.reduce((s, h) => s + h.today, 0),
    last7d: summaries.reduce((s, h) => s + h.last7d, 0),
    blocks: summaries.reduce((s, h) => s + h.blocks, 0),
    flags: summaries.reduce((s, h) => s + h.flags, 0),
    nudges: summaries.reduce((s, h) => s + h.nudges, 0),
  };

  return { summaries, totals };
}
