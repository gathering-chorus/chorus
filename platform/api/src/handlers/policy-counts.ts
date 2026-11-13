/**
 * GET /api/chorus/policies/counts?gate=arch — each policy of a gate with today's count, worst
 * first (#4963 AC4). Several gates make one table: ?gate=quality,data,product,code (#4964 AC4). The page loom/policy-counts.html reads it same-origin, so Jeff can open it
 * from his phone on the house wifi (not localhost).
 *
 * The rows are the door's (:3360 /v1/policies/policies). The counts are measured, not stored:
 * the route runs this tree's policy-validate for the gate (the same checks the demo gates and
 * the hourly sensor run) and caches the answer for ten minutes.
 *
 * Before a card's land, the door still holds the old rows; a policy-checks-*.json plan in this
 * tree (the file the card's land-<card>-rows will write) fills in the check fields a door row
 * lacks, and the row says pendingLand. After the land the door holds the same values and the
 * plan changes nothing. A door that does not answer is a 503, never a file (#4508).
 */

export interface PolicyRow {
  name: string;
  label?: string;
  comment?: string;
  gate?: string;
  status?: string;
  baseline?: string;
  checkKind?: string;
  checkSpec?: string;
  violationFixture?: string;
}

export interface Measured {
  name: string;
  verdict: 'pass' | 'fail' | 'broken';
  count: number | null;
  baseline: number | null;
  reason?: string;
}

export interface CountRow extends Measured {
  gate: string;
  comment: string;
  narrowed: boolean;
  pendingLand: boolean;
}

export interface PolicyCountsDeps {
  fetchPolicies: () => Promise<PolicyRow[]>;
  loadPlans: () => PolicyRow[];
  runValidate: (gate: string, rows: PolicyRow[]) => Promise<string>;
  now?: () => number;
}

/** Door rows, with a pending plan's check fields where the door row has no check yet. */
export function overlay(door: PolicyRow[], plans: PolicyRow[]): Array<PolicyRow & { pendingLand: boolean }> {
  const plan = new Map(plans.map((p) => [p.name, p]));
  return door.map((row) => {
    const p = plan.get(row.name);
    if (!p || (row.checkKind ?? '') !== '') return { ...row, pendingLand: false };
    return {
      ...row,
      status: p.status ?? row.status,
      baseline: p.baseline ?? row.baseline,
      checkKind: p.checkKind ?? row.checkKind,
      checkSpec: p.checkSpec ?? row.checkSpec,
      violationFixture: p.violationFixture ?? row.violationFixture,
      comment: p.comment ?? row.comment,
      pendingLand: true,
    };
  });
}

const LINE = /^(PASS|FAIL) (\S+) (\d+) (?:<=|>) (\d+)/;
const BROKEN = /^BROKEN (\S+): (.*)$/;

/** policy-validate's lines → one measurement per policy. */
export function parseValidate(stdout: string): Measured[] {
  const out: Measured[] = [];
  for (const line of stdout.split('\n')) {
    const m = LINE.exec(line);
    if (m) {
      out.push({ name: m[2], verdict: m[1] === 'PASS' ? 'pass' : 'fail', count: Number(m[3]), baseline: Number(m[4]) });
      continue;
    }
    const b = BROKEN.exec(line);
    if (b) out.push({ name: b[1], verdict: 'broken', count: null, baseline: null, reason: b[2] });
  }
  return out;
}

/** Worst first: could not measure, then over baseline, then by count. */
export function worstFirst(rows: CountRow[]): CountRow[] {
  const rank = { broken: 0, fail: 1, pass: 2 } as const;
  return [...rows].sort((a, b) => rank[a.verdict] - rank[b.verdict] || (b.count ?? 0) - (a.count ?? 0) || a.name.localeCompare(b.name));
}

interface Cached { at: number; body: unknown }
const cache = new Map<string, Cached>();
const TTL_MS = 10 * 60 * 1000;

export async function fetchPolicyCounts(gate: string, deps: PolicyCountsDeps): Promise<{ status: number; body: unknown }> {
  if (!/^[a-z]+(,[a-z]+)*$/.test(gate)) return { status: 400, body: { error: 'gate must be a word or words with commas, e.g. arch or quality,data' } };
  const gates = new Set(gate.split(','));
  const now = (deps.now ?? Date.now)();
  const hit = cache.get(gate);
  if (hit && now - hit.at < TTL_MS) return { status: 200, body: hit.body };

  let door: PolicyRow[];
  try {
    door = await deps.fetchPolicies();
  } catch (e) {
    return { status: 503, body: { error: `the policies door did not answer: ${(e as Error).message}` } };
  }
  const rows = overlay(door.filter((r) => gates.has(r.gate ?? '')), deps.loadPlans()).filter((r) => (r.checkKind ?? '') !== '');
  const measured = new Map(parseValidate(await deps.runValidate(gate, rows)).map((m) => [m.name, m]));
  const table = worstFirst(rows.map((r) => {
    const m = measured.get(r.name) ?? { name: r.name, verdict: 'broken' as const, count: null, baseline: null, reason: 'not measured' };
    const comment = r.comment ?? '';
    return { ...m, gate: r.gate ?? '', comment, narrowed: /\bNarrowed\b/.test(comment), pendingLand: r.pendingLand };
  }));
  const body = { gate, measuredAt: new Date(now).toISOString(), policies: table };
  cache.set(gate, { at: now, body });
  return { status: 200, body };
}

export function clearPolicyCountsCache(): void {
  cache.clear();
}
