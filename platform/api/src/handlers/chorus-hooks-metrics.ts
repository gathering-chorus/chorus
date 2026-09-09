/**
 * GET /api/chorus/hooks/metrics — 7-day hook decision aggregate (#2188).
 *
 * Dependencies injected:
 *   readLog — () => string | null — returns hooks.log contents or null
 *   now     — () => number — epoch ms (for cutoff + generatedAt)
 *
 * Behavior:
 *   - null → 503
 *   - Parse lines: "timestamp | hook_type | tool | role | module | decision | duration | session_id | context"
 *   - Filter to last 7 days by date prefix comparison
 *   - Skip rows where module is empty, '-', 'none', or decision is 'enter'
 *   - Count allow/deny/warn/total per module
 *   - enforcementPercent = modules with any deny / total modules * 100
 *   - Throw during parse → 500
 */
import type { FetchResult } from './codebase-topology';

export interface ChorusHooksMetricsDeps {
  readLog: () => string | null;
  now?: () => number;
}

type ModuleStats = { allow: number; deny: number; warn: number; total: number };
type ModuleMap = Partial<Record<string, ModuleStats>>;

function cutoffDateStr(now: () => number): string {
  const cutoff = new Date(now());
  cutoff.setDate(cutoff.getDate() - 7);
  return cutoff.toISOString().slice(0, 10);
}

// #4131 — the daemon writes one JSON object per line ({timestamp, hook, tool,
// role, module, decision, latency_ms, ...}); the pipe form below is what the
// log held when #2277 was written. This read only ever parsed the pipe form,
// so once the last pipe lines rotated out (2026-09-09) it answered
// totalDecisions: 0 over a log with 1,600 decisions in the window, and
// hooks-metrics-api.bats went red on "no modules". Same skip rules both ways.
function parseJsonLine(line: string, cutoffStr: string): { module: string; decision: string } | null {
  let o: { timestamp?: unknown; module?: unknown; decision?: unknown };
  try { o = JSON.parse(line) as typeof o; } catch { return null; }
  const ts = typeof o.timestamp === 'string' ? o.timestamp : '';
  if (ts.slice(0, 10) < cutoffStr) return null;
  const module = typeof o.module === 'string' ? o.module : '';
  const decision = typeof o.decision === 'string' ? o.decision.toLowerCase() : '';
  if (!module || module === '-' || module === 'none' || decision === 'enter') return null;
  return { module, decision };
}
function parseLogLine(line: string, cutoffStr: string): { module: string; decision: string } | null {
  if (line.startsWith('{')) return parseJsonLine(line, cutoffStr);
  const parts = line.split('|').map((s) => s.trim());
  if (parts.length < 6) return null;
  if (parts[0].slice(0, 10) < cutoffStr) return null;
  const module = parts[4];
  const decision = parts[5].toLowerCase();
  if (!module || module === '-' || module === 'none' || decision === 'enter') return null;
  return { module, decision };
}

function applyDecision(stats: ModuleStats, decision: string): void {
  stats.total++;
  if (decision === 'allow') stats.allow++;
  else if (decision === 'deny' || decision === 'block') stats.deny++;
  else if (decision === 'warn') stats.warn++;
}

function aggregateLines(lines: string[], cutoffStr: string): { modules: ModuleMap; totalDecisions: number } {
  const modules: ModuleMap = {};
  let totalDecisions = 0;
  for (const line of lines) {
    const parsed = parseLogLine(line, cutoffStr);
    if (!parsed) continue;
    let stats = modules[parsed.module];
    if (!stats) { stats = { allow: 0, deny: 0, warn: 0, total: 0 }; modules[parsed.module] = stats; }
    applyDecision(stats, parsed.decision);
    totalDecisions++;
  }
  return { modules, totalDecisions };
}

export function fetchChorusHooksMetrics(deps: ChorusHooksMetricsDeps): FetchResult {
  const now = deps.now ?? Date.now;
  const raw = deps.readLog();
  if (raw === null) {
    return { status: 503, body: { error: 'hooks.log not found' } };
  }
  try {
    const { modules, totalDecisions } = aggregateLines(raw.trim().split('\n'), cutoffDateStr(now));
    const enforcedModules = Object.entries(modules).filter(([, v]) => v !== undefined && v.deny > 0).length;
    const totalModules = Object.keys(modules).length;
    const enforcementPercent = totalModules > 0 ? Math.round((enforcedModules / totalModules) * 100) : 0;

    return {
      status: 200,
      body: {
        totalDecisions,
        totalModules,
        enforcedModules,
        enforcementPercent,
        periodDays: 7,
        modules,
        generatedAt: new Date(now()).toISOString(),
      },
    };
  } catch {
    return { status: 500, body: { error: 'Failed to parse hooks log' } };
  }
}
