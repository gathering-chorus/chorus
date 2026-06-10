/**
 * GET /api/chorus/hooks/friction — ranked hook-friction view (#3280).
 *
 * Which hook blocks whom, how often, per role — the aggregation hook.decision
 * data never had. Reads hooks.log (the #3252 JSON-lines format; AC2: no new
 * store) and ranks modules by DENY/BLOCK/WARN volume inside a sliding window,
 * so "accept_gate blocked Kade 72× in 12h" is a queryable fact instead of 470
 * log lines nobody adds up.
 *
 * Dependencies injected (same seam as chorus-hooks-metrics):
 *   readLog     — () => string | null — hooks.log contents or null
 *   now         — () => epoch ms
 *   windowHours — aggregation window, default 12
 *
 * Counting rules:
 *   - allow is not friction; module none/empty is allow-path noise → skipped
 *   - deny and block are one refusal class (counted as deny)
 *   - warn counts separately (friction, but not a stop)
 *   - legacy pipe-format / garbage lines are skipped, never throw
 */
import type { FetchResult } from './codebase-topology';

export interface HookFrictionDeps {
  readLog: () => string | null;
  now?: () => number;
  windowHours?: number;
}

type HookRow = {
  module: string;
  total: number;
  deny: number;
  warn: number;
  byRole: Record<string, number>;
};

function parseJsonLine(line: string): { module: string; role: string; decision: string; tsMs: number } | null {
  const t = line.trim();
  if (!t.startsWith('{')) return null;
  try {
    const d = JSON.parse(t) as Record<string, unknown>;
    const module = typeof d.module === 'string' ? d.module : '';
    const decision = typeof d.decision === 'string' ? d.decision.toLowerCase() : '';
    const role = typeof d.role === 'string' && d.role ? d.role : 'unknown';
    const tsMs = Date.parse(typeof d.timestamp === 'string' ? d.timestamp : '');
    if (!module || module === 'none' || Number.isNaN(tsMs)) return null;
    return { module, role, decision, tsMs };
  } catch {
    return null;
  }
}

export function fetchHookFriction(deps: HookFrictionDeps): FetchResult {
  const now = deps.now ?? Date.now;
  const windowHours = deps.windowHours ?? 12;
  const raw = deps.readLog();
  if (raw === null) {
    return { status: 503, body: { error: 'hooks.log not found' } };
  }
  const cutoff = now() - windowHours * 60 * 60 * 1000;
  const byModule = new Map<string, HookRow>();
  let totalFriction = 0;

  for (const line of raw.split('\n')) {
    const p = parseJsonLine(line);
    if (!p || p.tsMs < cutoff) continue;
    const isDeny = p.decision === 'deny' || p.decision === 'block';
    const isWarn = p.decision === 'warn';
    if (!isDeny && !isWarn) continue;

    let row = byModule.get(p.module);
    if (!row) {
      row = { module: p.module, total: 0, deny: 0, warn: 0, byRole: {} };
      byModule.set(p.module, row);
    }
    row.total++;
    if (isDeny) row.deny++;
    else row.warn++;
    row.byRole[p.role] = (row.byRole[p.role] ?? 0) + 1;
    totalFriction++;
  }

  // Rank: volume first; at equal volume a deny outranks a warn (refusals are
  // harder friction than warnings); name only as the final stable tie-break.
  const hooks = [...byModule.values()].sort(
    (a, b) => b.total - a.total || b.deny - a.deny || a.module.localeCompare(b.module),
  );
  return {
    status: 200,
    body: {
      windowHours,
      totalFriction,
      hooks,
      generatedAt: new Date(now()).toISOString(),
    },
  };
}
