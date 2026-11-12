// @test-type: unit — the handler with injected door, plan and validator; no server, no store.
// @domain: policies
/**
 * #4963 AC4 — the policy-counts table: each policy of a gate with today's count, worst first,
 * every narrowed rule marked, the door's rows (with a pending plan only where the door has no
 * check yet), and a door that does not answer is a 503 — never a file.
 */
import {
  clearPolicyCountsCache, fetchPolicyCounts, overlay, parseValidate, type PolicyRow,
} from '../src/handlers/policy-counts';

const door: PolicyRow[] = [
  { name: 'arch-a', gate: 'arch', comment: 'Rule A. Narrowed: counts x only. (#4963)', checkKind: 'command', checkSpec: 'x', baseline: '5' },
  { name: 'arch-b', gate: 'arch', comment: 'Rule B, words only.' },
  { name: 'arch-c', gate: 'arch', comment: 'Rule C. Checked as written: y.', checkKind: 'command', checkSpec: 'y', baseline: '0' },
  { name: 'data-d', gate: 'data', comment: 'Other gate.', checkKind: 'sparql', checkSpec: 'q', baseline: '0' },
];
const plan: PolicyRow[] = [
  { name: 'arch-b', comment: 'Rule B. Narrowed: counts z. (#4963)', checkKind: 'command', checkSpec: 'z', baseline: '40', status: 'ratchet' },
  { name: 'arch-a', comment: 'a plan must not override a check the door already holds', checkKind: 'command', checkSpec: 'other', baseline: '99' },
];
const validate = 'PASS arch-a 5 <= 5\nFAIL arch-c 2 > 0 (arch)\nPASS arch-b 40 <= 40\npolicy-validate: 3 checks, 1 above baseline, 0 could not run (gate arch)\n';

beforeEach(() => clearPolicyCountsCache());

function deps(over: Partial<Parameters<typeof fetchPolicyCounts>[1]> = {}) {
  const seen: PolicyRow[][] = [];
  return {
    seen,
    d: {
      fetchPolicies: async () => door,
      loadPlans: () => plan,
      runValidate: async (_g: string, rows: PolicyRow[]) => { seen.push(rows); return validate; },
      now: () => Date.UTC(2026, 10, 12, 21, 50),
      ...over,
    },
  };
}

test('the gate\'s checked policies, worst first: over the ceiling, then by count', async () => {
  const { d } = deps();
  const r = await fetchPolicyCounts('arch', d);
  expect(r.status).toBe(200);
  const rows = (r.body as { policies: Array<{ name: string; count: number; verdict: string }> }).policies;
  expect(rows.map((p) => [p.name, p.count, p.verdict])).toEqual([
    ['arch-c', 2, 'fail'], ['arch-b', 40, 'pass'], ['arch-a', 5, 'pass'],
  ]);
});

test('every narrowed rule is marked, and its own note travels with it (AC3 on the table)', async () => {
  const { d } = deps();
  const rows = ((await fetchPolicyCounts('arch', d)).body as { policies: Array<{ name: string; narrowed: boolean; comment: string }> }).policies;
  const by = Object.fromEntries(rows.map((p) => [p.name, p]));
  expect(by['arch-a'].narrowed).toBe(true);
  expect(by['arch-b'].narrowed).toBe(true);
  expect(by['arch-c'].narrowed).toBe(false);
  expect(by['arch-b'].comment).toContain('Narrowed: counts z');
});

test('a pending plan fills in only a door row that has no check yet, and says so', () => {
  const rows = overlay(door, plan);
  const b = rows.find((r) => r.name === 'arch-b')!;
  const a = rows.find((r) => r.name === 'arch-a')!;
  expect([b.pendingLand, b.checkSpec, b.baseline]).toEqual([true, 'z', '40']);
  expect([a.pendingLand, a.checkSpec, a.baseline]).toEqual([false, 'x', '5']);
  expect(rows.find((r) => r.name === 'data-d')!.pendingLand).toBe(false);
});

test('negative proof: a plan never adds a policy the door does not hold', () => {
  const rows = overlay(door, [{ name: 'arch-ghost', checkKind: 'command', checkSpec: 'g', baseline: '0' }]);
  expect(rows.map((r) => r.name)).not.toContain('arch-ghost');
});

test('only the asked gate is measured', async () => {
  const { d, seen } = deps();
  await fetchPolicyCounts('arch', d);
  expect(seen[0].map((r) => r.name).sort()).toEqual(['arch-a', 'arch-b', 'arch-c']);
});

test('negative proof: a check that could not run shows as could-not-measure, first — never as 0', async () => {
  const { d } = deps({ runValidate: async () => 'PASS arch-a 5 <= 5\nBROKEN arch-b: store unreachable\n' });
  const rows = ((await fetchPolicyCounts('arch', d)).body as { policies: Array<{ name: string; verdict: string; count: number | null; reason?: string }> }).policies;
  expect(rows[0].verdict).toBe('broken');
  expect(rows.filter((p) => p.verdict === 'broken').map((p) => p.count)).toEqual([null, null]);
  expect(rows.find((p) => p.name === 'arch-c')!.reason).toBe('not measured');
});

test('negative proof: a door that does not answer is a 503, not a table from the plan file', async () => {
  const { d } = deps({ fetchPolicies: async () => { throw new Error('ECONNREFUSED'); } });
  const r = await fetchPolicyCounts('arch', d);
  expect(r.status).toBe(503);
});

test('negative proof: a gate that is not a word is refused before anything runs', async () => {
  const { d, seen } = deps();
  expect((await fetchPolicyCounts('arch; rm -rf', d)).status).toBe(400);
  expect(seen).toHaveLength(0);
});

test('the answer is cached for ten minutes, then measured again', async () => {
  let t = Date.UTC(2026, 10, 12, 21, 50);
  const { d, seen } = deps({ now: () => t });
  await fetchPolicyCounts('arch', d);
  t += 9 * 60 * 1000;
  await fetchPolicyCounts('arch', d);
  expect(seen).toHaveLength(1);
  t += 2 * 60 * 1000;
  await fetchPolicyCounts('arch', d);
  expect(seen).toHaveLength(2);
});

test('parseValidate reads PASS, FAIL and BROKEN lines and ignores the summary', () => {
  expect(parseValidate(validate + 'BROKEN arch-x: no baseline\n').map((m) => [m.name, m.verdict, m.count])).toEqual([
    ['arch-a', 'pass', 5], ['arch-c', 'fail', 2], ['arch-b', 'pass', 40], ['arch-x', 'broken', null],
  ]);
});
