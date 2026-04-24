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

export function fetchChorusHooksMetrics(deps: ChorusHooksMetricsDeps): FetchResult {
  const now = deps.now ?? Date.now;

  const raw = deps.readLog();
  if (raw === null) {
    return { status: 503, body: { error: 'hooks.log not found' } };
  }

  try {
    const lines = raw.trim().split('\n');

    const cutoff = new Date(now());
    cutoff.setDate(cutoff.getDate() - 7);
    const cutoffStr = cutoff.toISOString().slice(0, 10);

    type ModuleStats = { allow: number; deny: number; warn: number; total: number };
    const modules: Partial<Record<string, ModuleStats>> = {};
    let totalDecisions = 0;

    for (const line of lines) {
      const parts = line.split('|').map((s) => s.trim());
      if (parts.length < 6) continue;

      const timestamp = parts[0].slice(0, 10);
      if (timestamp < cutoffStr) continue;

      const moduleName = parts[4];
      const decision = parts[5].toLowerCase();

      if (!moduleName || moduleName === '-' || moduleName === 'none' || decision === 'enter') continue;

      let m = modules[moduleName];
      if (!m) { m = { allow: 0, deny: 0, warn: 0, total: 0 }; modules[moduleName] = m; }

      m.total++;
      totalDecisions++;

      if (decision === 'allow') m.allow++;
      else if (decision === 'deny' || decision === 'block') m.deny++;
      else if (decision === 'warn') m.warn++;
    }

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
