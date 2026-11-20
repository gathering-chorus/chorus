// @test-type: unit
// @domain: policies
// @card: #5021
/**
 * #5021 AC5 — the Conformance fold on /athena/domain.html?d=<name> reads
 * GET /api/chorus/policies/counts?domain=<name>: every policy that applies to the domain (global, or
 * appliesTo it) with its last `policy-validate --domain` result and when it ran, from the policies
 * door and the lint's own run. NEGATIVE: with one check red, that policy is red with its count.
 */
import * as fs from 'fs';
import * as path from 'path';
import { fetchDomainConformance, clearPolicyCountsCache, type PolicyRow } from '../../src/handlers/policy-counts';

const door: PolicyRow[] = [
  { name: 'domain-has-owner', checkKind: 'sparql', checkSpec: 'q {domain}', baseline: '0', gate: 'product', label: 'An owner' },
  { name: 'domain-referential-integrity', checkKind: 'sparql', checkSpec: 'q {domain}', baseline: '77', gate: 'data', label: 'Links reach rows' },
  { name: 'only-principles', checkKind: 'command', checkSpec: 'x', baseline: '0', gate: 'code', appliesTo: ['chorus:principles'] },
  { name: 'only-values', checkKind: 'command', checkSpec: 'x', baseline: '0', gate: 'code', appliesTo: 'chorus:values' },
  { name: 'retired-one', checkKind: 'command', checkSpec: 'x', baseline: '0', gate: 'code', status: 'retired' },
  { name: 'words-only', label: 'a rule to read' },
];

const RED = [
  'PASS domain-has-owner 0 <= 0',
  'FAIL domain-referential-integrity 2 > 0 (data)',
  '  rule: Links reach rows',
  '  - principles: a b chorus#xp-humanity',
  '  - principles: a b chorus#xp-flow',
  'PASS only-principles 0 <= 0',
  'policy-validate: 3 checks, 1 above baseline, 0 could not run (domain principles)',
].join('\n');

function deps(stdout: string, seen: { rows?: string[]; args?: string[] } = {}) {
  return {
    fetchPolicies: async () => door,
    loadPlans: () => [],
    runValidate: async (_g: string, rows: PolicyRow[], args?: string[]) => { seen.rows = rows.map((r) => r.name); seen.args = args; return stdout; },
    now: () => Date.parse('2026-11-20T23:30:00Z'),
  };
}

beforeEach(() => clearPolicyCountsCache());

test('runs the domain lint over the policies that apply: global and scoped to it, nothing else', async () => {
  const seen: { rows?: string[]; args?: string[] } = {};
  const r = await fetchDomainConformance('principles', deps(RED, seen));
  expect(r.status).toBe(200);
  expect(seen.args).toEqual(['--domain', 'principles']);
  expect((seen.rows ?? []).sort()).toEqual(['domain-has-owner', 'domain-referential-integrity', 'only-principles']);
  const body = r.body as { domain: string; measuredAt: string };
  expect(body.domain).toBe('principles');
  expect(body.measuredAt).toBe('2026-11-20T23:30:00.000Z');
});

test('NEGATIVE: with one check red, the fold shows that policy red with its count and violations', async () => {
  const body = (await fetchDomainConformance('principles', deps(RED))).body as {
    policies: Array<{ name: string; verdict: string; count: number; baseline: number; violations?: string[]; scope: string }>;
  };
  const red = body.policies.find((p) => p.name === 'domain-referential-integrity')!;
  expect([red.verdict, red.count, red.baseline]).toEqual(['fail', 2, 0]);
  expect(red.violations).toEqual(['principles: a b chorus#xp-humanity', 'principles: a b chorus#xp-flow']);
  expect(body.policies[0].name).toBe('domain-referential-integrity'); // worst first
  expect(body.policies.find((p) => p.name === 'only-principles')!.scope).toBe('domain');
  expect(body.policies.find((p) => p.name === 'domain-has-owner')!.scope).toBe('global');
});

test('control: all green reads green', async () => {
  const green = RED.replace('FAIL domain-referential-integrity 2 > 0 (data)', 'PASS domain-referential-integrity 0 <= 0').replace(/^ {2}.*$/gm, '');
  const body = (await fetchDomainConformance('principles', deps(green))).body as { policies: Array<{ verdict: string }> };
  expect(body.policies.every((p) => p.verdict === 'pass')).toBe(true);
});

test('NEGATIVE: a policy the run did not report is "could not run", never green', async () => {
  const body = (await fetchDomainConformance('principles', deps('PASS domain-has-owner 0 <= 0'))).body as { policies: Array<{ name: string; verdict: string }> };
  expect(body.policies.find((p) => p.name === 'only-principles')!.verdict).toBe('broken');
});

test('NEGATIVE: a bad domain name is a 400; a door that does not answer is a 503', async () => {
  expect((await fetchDomainConformance('princ;x', deps(RED))).status).toBe(400);
  const down = { ...deps(RED), fetchPolicies: async () => { throw new Error('ECONNREFUSED'); } };
  expect((await fetchDomainConformance('principles', down)).status).toBe(503);
});

test('the page draws the fold from that route, for any ?d=, and shows when it ran', () => {
  const page = fs.readFileSync(path.join(__dirname, '../../public/athena/domain.html'), 'utf8');
  expect(page).toContain('renderConformance(d)');
  expect(page).toContain('/api/chorus/policies/counts?domain=${encodeURIComponent(d)}');
  expect(page).toContain('measured ${esc(b.measuredAt');
  expect(page).not.toMatch(/renderConformance\('principles'\)/);
});
