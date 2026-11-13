// @test-type: unit — the handler with injected door, plan and validator; no server, no store.
// @domain: policies
/**
 * #4964 AC4 — one table for several gates: ?gate=quality,data,product,code measures those
 * gates' checked policies together, worst first, each row naming its gate. A gate list that is
 * not words with commas is refused before anything runs.
 */
import { clearPolicyCountsCache, fetchPolicyCounts, overlay, type PolicyRow } from '../src/handlers/policy-counts';

const door: PolicyRow[] = [
  { name: 'qual-a', gate: 'quality', comment: 'Narrowed: a.', checkKind: 'command', checkSpec: 'a', baseline: '6' },
  { name: 'data-b', gate: 'data', comment: 'Narrowed: b.', checkKind: 'sparql', checkSpec: 'b', baseline: '64' },
  { name: 'prod-c', gate: 'product', comment: 'Narrowed: c.', checkKind: 'command', checkSpec: 'c', baseline: '0' },
  { name: 'arch-d', gate: 'arch', comment: 'Not asked.', checkKind: 'command', checkSpec: 'd', baseline: '1' },
];
const validate = 'PASS qual-a 6 <= 6\nPASS data-b 64 <= 64\nBROKEN prod-c: exit 2: cards: down\n';

beforeEach(() => clearPolicyCountsCache());

function deps() {
  const seen: PolicyRow[][] = [];
  const ran: string[] = [];
  return {
    seen, ran,
    d: {
      fetchPolicies: async () => door,
      loadPlans: () => [],
      runValidate: async (g: string, rows: PolicyRow[]) => { ran.push(g); seen.push(rows); return validate; },
      now: () => Date.UTC(2026, 10, 13, 1, 30),
    },
  };
}

test('several gates make one table, worst first, each row naming its gate', async () => {
  const { d, seen, ran } = deps();
  const r = await fetchPolicyCounts('quality,data,product', d);
  expect(r.status).toBe(200);
  const rows = (r.body as { policies: Array<{ name: string; gate: string; verdict: string; count: number | null }> }).policies;
  expect(rows.map((p) => [p.name, p.gate, p.verdict, p.count])).toEqual([
    ['prod-c', 'product', 'broken', null], ['data-b', 'data', 'pass', 64], ['qual-a', 'quality', 'pass', 6],
  ]);
  expect(ran).toEqual(['quality,data,product']);
  expect(seen[0].map((x) => x.name).sort()).toEqual(['data-b', 'prod-c', 'qual-a']);
});

test('negative proof: a gate not asked for is not measured', async () => {
  const { d, seen } = deps();
  await fetchPolicyCounts('quality,data,product', d);
  expect(seen[0].some((x) => x.name === 'arch-d')).toBe(false);
});

test('negative proof: a gate list that is not words with commas is refused before anything runs', async () => {
  for (const bad of ['quality,', ',data', 'quality;rm', 'quality, data', 'Quality']) {
    const { d, ran } = deps();
    const r = await fetchPolicyCounts(bad, d);
    expect(r.status).toBe(400);
    expect(ran).toEqual([]);
  }
});

// the rename window: the door still names the retired script, this tree's plan names the new one
const oldSpec = 'p=platform/scripts/arch-policy-count; "$p" arch-x';
const newSpec = 'p=platform/scripts/policy-count; "$p" arch-x';
const doorOld: PolicyRow[] = [{ name: 'arch-x', gate: 'arch', checkKind: 'command', checkSpec: oldSpec, baseline: '3' }];
const planNew: PolicyRow[] = [{ name: 'arch-x', checkKind: 'command', checkSpec: newSpec, baseline: '3' }];
const hasScript = (r: PolicyRow) => !/arch-policy-count/.test(r.checkSpec ?? '');

test('a door check naming a script this tree lacks takes the tree plan, marked pendingLand', () => {
  const [r] = overlay(doorOld, planNew, hasScript);
  expect(r.checkSpec).toBe(newSpec);
  expect(r.pendingLand).toBe(true);
});

test('negative proof: a runnable door check still wins over the plan', () => {
  const [r] = overlay(doorOld, planNew, () => true);
  expect(r.checkSpec).toBe(oldSpec);
  expect(r.pendingLand).toBe(false);
});

test('negative proof: an unrunnable door check with no plan stays as the door holds it (measured as could-not-run, never hidden)', () => {
  const [r] = overlay(doorOld, [], hasScript);
  expect(r.checkSpec).toBe(oldSpec);
  expect(r.pendingLand).toBe(false);
});
