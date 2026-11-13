// @test-type: unit — the handler with injected door, plan and validator; no server, no store.
// @domain: policies
/**
 * #4964 AC4 — one table for several gates: ?gate=quality,data,product,code measures those
 * gates' checked policies together, worst first, each row naming its gate. A gate list that is
 * not words with commas is refused before anything runs.
 */
import { clearPolicyCountsCache, fetchPolicyCounts, type PolicyRow } from '../src/handlers/policy-counts';

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
