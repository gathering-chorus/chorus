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

import { isDomainName } from './util';

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
  /** #5020: the domains a policy is scoped to; none means every domain */
  appliesTo?: string | string[];
}

export interface Measured {
  name: string;
  verdict: 'pass' | 'fail' | 'broken';
  count: number | null;
  baseline: number | null;
  reason?: string;
  /** a domain lint lists the first violations under each red (#5021) */
  violations?: string[];
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
  /** false when the row's check names a script missing from this tree (default: all runnable) */
  runnable?: (row: PolicyRow) => boolean;
  runValidate: (gate: string, rows: PolicyRow[], args?: string[]) => Promise<string>;
  now?: () => number;
}

/**
 * Door rows, with a pending plan's check fields where the door row has no check yet, or where
 * its check cannot run in this tree (it names a script this tree's land retires: #4964 moved
 * the arch checks from arch-policy-count to policy-count, and until the land writes the rows the
 * door still names the old script).
 */
export function overlay(
  door: PolicyRow[], plans: PolicyRow[], runnable: (row: PolicyRow) => boolean = () => true,
): Array<PolicyRow & { pendingLand: boolean }> {
  const plan = new Map(plans.map((p) => [p.name, p]));
  return door.map((row) => {
    const p = plan.get(row.name);
    if (!p || ((row.checkKind ?? '') !== '' && runnable(row))) return { ...row, pendingLand: false };
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

/** One PASS/FAIL or BROKEN line → its measurement; anything else → null. */
function measuredLine(line: string): Measured | null {
  const m = LINE.exec(line);
  if (m) return { name: m[2], verdict: m[1] === 'PASS' ? 'pass' : 'fail', count: Number(m[3]), baseline: Number(m[4]) };
  const b = BROKEN.exec(line);
  return b ? { name: b[1], verdict: 'broken', count: null, baseline: null, reason: b[2] } : null;
}

/** policy-validate's lines → one measurement per policy. A domain lint lists a red's
 *  violations under it as "  - <violation>" (#5020); they ride on that red. */
export function parseValidate(stdout: string): Measured[] {
  const out: Measured[] = [];
  for (const line of stdout.split('\n')) {
    const m = measuredLine(line);
    if (m) { out.push(m); continue; }
    const v = /^ {2}- (.*)$/.exec(line);
    const last = out.length ? out[out.length - 1] : undefined;
    if (v && last?.verdict === 'fail') (last.violations ??= []).push(v[1]);
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
  if (!gate.split(',').every((g) => /^[a-z]+$/.test(g))) return { status: 400, body: { error: 'gate must be a word or words with commas, e.g. arch or quality,data' } };
  const gates = new Set(gate.split(','));
  const now = (deps.now ?? Date.now)();
  const hit = cache.get(`gate:${gate}`);
  if (hit && now - hit.at < TTL_MS) return { status: 200, body: hit.body };

  let door: PolicyRow[];
  try {
    door = await deps.fetchPolicies();
  } catch (e) {
    return { status: 503, body: { error: `the policies door did not answer: ${(e as Error).message}` } };
  }
  const rows = overlay(door.filter((r) => gates.has(r.gate ?? '')), deps.loadPlans(), deps.runnable).filter((r) => (r.checkKind ?? '') !== '');
  const measured = new Map(parseValidate(await deps.runValidate(gate, rows)).map((m) => [m.name, m]));
  const table = worstFirst(rows.map((r) => {
    const m = measured.get(r.name) ?? { name: r.name, verdict: 'broken' as const, count: null, baseline: null, reason: 'not measured' };
    const comment = r.comment ?? '';
    return { ...m, gate: r.gate ?? '', comment, narrowed: /\bNarrowed\b/.test(comment), pendingLand: r.pendingLand };
  }));
  const body = { gate, measuredAt: new Date(now).toISOString(), policies: table };
  cache.set(`gate:${gate}`, { at: now, body });
  return { status: 200, body };
}

/** The domain names a row is scoped to (chorus:x, its IRI, or x); none = every domain. */
export function appliesTo(row: PolicyRow): string[] {
  const a = row.appliesTo ?? [];
  return (Array.isArray(a) ? a : [a]).filter(Boolean).map((x) => String(x).split(/[#:/]/).pop() as string);
}

/**
 * #5021 — GET /api/chorus/policies/counts?domain=<name>: the domain's lint, for the Conformance
 * fold on /athena/domain.html. The rows are the door's (with a pending land's plan, as above),
 * every check that applies to the domain: global (no appliesTo) or scoped to it. The verdicts
 * are this tree's `policy-validate --domain <name>` (a {domain} check holds at 0 for one
 * domain), cached ten minutes, with the time it ran. A door that does not answer is a 503.
 * One run per domain at a time: a second view while the lint runs waits on the same run. A run
 * that measured nothing (it failed or timed out) is shown, never cached, so the next view runs again.
 */
const inflight = new Map<string, Promise<{ status: number; body: unknown }>>();

export async function fetchDomainConformance(domain: string, deps: PolicyCountsDeps): Promise<{ status: number; body: unknown }> {
  if (!isDomainName(domain)) return { status: 400, body: { error: 'domain must be a domain name, e.g. principles' } };
  const now = (deps.now ?? Date.now)();
  const key = `domain:${domain}`;
  const hit = cache.get(key);
  if (hit && now - hit.at < TTL_MS) return { status: 200, body: hit.body };
  const running = inflight.get(key);
  if (running) return running;
  const run = measureDomain(domain, key, now, deps).finally(() => inflight.delete(key));
  inflight.set(key, run);
  return run;
}

async function measureDomain(domain: string, key: string, now: number, deps: PolicyCountsDeps): Promise<{ status: number; body: unknown }> {
  let door: PolicyRow[];
  try {
    door = await deps.fetchPolicies();
  } catch (e) {
    return { status: 503, body: { error: `the policies door did not answer: ${(e as Error).message}` } };
  }
  const rows = overlay(door, deps.loadPlans(), deps.runnable)
    .filter((r) => (r.checkKind ?? '') !== '' && r.checkKind !== 'runtime' && r.status !== 'retired')
    .filter((r) => { const s = appliesTo(r); return s.length === 0 || s.includes(domain); });
  const measured = new Map(parseValidate(await deps.runValidate(domain, rows, ['--domain', domain])).map((m) => [m.name, m]));
  const table = worstFirst(rows.map((r) => {
    const m = measured.get(r.name) ?? { name: r.name, verdict: 'broken' as const, count: null, baseline: null, reason: 'not measured' };
    const comment = r.comment ?? '';
    return { ...m, gate: r.gate ?? '', comment, narrowed: /\bNarrowed\b/.test(comment), pendingLand: r.pendingLand,
      label: r.label ?? r.name, scope: appliesTo(r).length ? 'domain' : 'global' };
  }));
  const body = { domain, measuredAt: new Date(now).toISOString(), policies: table };
  if (measured.size > 0) cache.set(key, { at: now, body });
  return { status: 200, body };
}

export function clearPolicyCountsCache(): void {
  cache.clear();
  inflight.clear();
}
