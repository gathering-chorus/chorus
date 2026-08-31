/* eslint-disable security/detect-object-injection --
 * Bracket access is on tallies keyed by board bucket titles from our own
 * read-only Vikunja query and RANGE_MS union keys; no untrusted HTTP input
 * reaches a bracket-access sink (range is hasOwnProperty-gated). */
/**
 * GET /api/loom-analytics — the stranded #2116 instrument, migrated (#4036).
 *
 * Cumulative flow by status, throughput + daily series, per-role lead time
 * (avg/max hours over shipped cards), and the bottleneck status — computed
 * on read from the board (Jeff's 2026-08-31 Q1=A ruling: no observation
 * store; the board and spine ARE the record).
 *
 * Dependencies injected — readCards returns board rows; no DB in unit tests.
 */

export interface LoomCardRow {
  id: number;
  status: string;      // bucket title: Next / WIP / Done / ...
  owner: string;       // from owner:<role> label, '' when unowned
  created: string;     // ISO datetime
  done: boolean;
  doneAt: string;      // ISO datetime, '' when not done
}

export interface LoomAnalyticsDeps {
  readCards: () => LoomCardRow[];
  now?: () => number;
}

const RANGE_MS: Record<string, number> = {
  '7d': 7 * 24 * 3600 * 1000,
  '30d': 30 * 24 * 3600 * 1000,
  '90d': 90 * 24 * 3600 * 1000,
  all: Infinity,
};

export interface RoleFitness { role: string; shipped: number; avgLeadHours: number; maxLeadHours: number }

function countByStatus(rows: LoomCardRow[], openOnly: boolean): Record<string, number> {
  const out: Record<string, number> = {};
  for (const c of rows) {
    if (openOnly && c.done) continue;
    const s = c.status || 'Unstaged';
    out[s] = (out[s] ?? 0) + 1;
  }
  return out;
}

function dailySeries(done: LoomCardRow[]): Array<{ date: string; count: number }> {
  const byDay = new Map<string, number>();
  for (const c of done) {
    const day = c.doneAt.slice(0, 10);
    byDay.set(day, (byDay.get(day) ?? 0) + 1);
  }
  return [...byDay.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([date, count]) => ({ date, count }));
}

function roleFitnessRows(done: LoomCardRow[]): RoleFitness[] {
  const byRole = new Map<string, number[]>();
  for (const c of done) {
    if (!c.owner) continue;
    const leads = byRole.get(c.owner) ?? [];
    const h = (Date.parse(c.doneAt) - Date.parse(c.created)) / 3600000;
    if (!Number.isNaN(h) && h >= 0) leads.push(h);
    else leads.push(-1); // shipped but unmeasurable lead — counts as shipped, excluded from lead math
    byRole.set(c.owner, leads);
  }
  const rows: RoleFitness[] = [];
  for (const [role, all] of byRole) {
    const leads = all.filter((h) => h >= 0);
    const avg = leads.length ? leads.reduce((a, b) => a + b, 0) / leads.length : 0;
    rows.push({
      role,
      shipped: all.length,
      avgLeadHours: Math.round(avg * 10) / 10,
      maxLeadHours: leads.length ? Math.round(Math.max(...leads) * 10) / 10 : 0,
    });
  }
  return rows.sort((a, b) => b.shipped - a.shipped || a.role.localeCompare(b.role));
}

export function computeLoomAnalytics(rows: LoomCardRow[], range: string, nowMs: number): Record<string, unknown> {
  const win = RANGE_MS[range] ?? Infinity;
  const cutoff = win === Infinity ? -Infinity : nowMs - win;

  const doneInRange = rows.filter((c) => {
    if (!c.done || !c.doneAt) return false;
    const t = Date.parse(c.doneAt);
    return !Number.isNaN(t) && t >= cutoff && t <= nowMs;
  });

  const openCounts = countByStatus(rows, true);
  const sorted = Object.entries(openCounts).sort((a, b) => b[1] - a[1]);
  const worst = sorted.length > 0 ? sorted[0] : null;

  return {
    range,
    totalCards: rows.length,
    flow: countByStatus(rows, false),
    throughput: doneInRange.length,
    dailyThroughput: dailySeries(doneInRange),
    roleFitness: roleFitnessRows(doneInRange),
    bottleneck: worst ? { status: worst[0], count: worst[1] } : null,
    queriedAt: new Date(nowMs).toISOString(),
  };
}

export function fetchLoomAnalytics(deps: LoomAnalyticsDeps, range: string): { status: number; body: Record<string, unknown> } {
  const r = Object.prototype.hasOwnProperty.call(RANGE_MS, range) ? range : 'all';
  try {
    const rows = deps.readCards();
    return { status: 200, body: computeLoomAnalytics(rows, r, (deps.now ?? Date.now)()) };
  } catch (e) {
    return { status: 500, body: { error: `loom-analytics: ${e instanceof Error ? e.message : String(e)}` } };
  }
}
