// @test-type: unit
// @domain: policies
// @card: #5021
/**
 * #5021 AC5 — the Conformance fold on /athena/domain.html?d=<name> reads
 * GET /api/chorus/policies/counts?domain=<name>: every policy that applies to the domain (global, or
 * appliesTo it) with its last `policy-validate --domain` result and when it ran, from the policies
 * door and the lint's own run. NEGATIVE: with one check red, that policy is red with its count.
 *
 * #5044 — the fold's read answers the KEPT result at once and never waits for the lint (Jeff's
 * phone, 2026-11-25: "Cannot reach the policies endpoint"; the lint ran inside the request, 27 s,
 * behind a door that gives up at 30). Every case reads the way the page does: the read, the
 * check that read started (waited on with domainCheckSettled), then the read again.
 */
import {
  fetchDomainConformance, domainCheckSettled, clearPolicyCountsCache, type PolicyRow,
} from '../../src/handlers/policy-counts';

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

const T0 = Date.parse('2026-11-20T23:30:00Z');
const MIN = 60 * 1000;

function deps(stdout: string, seen: { rows?: string[]; args?: string[] } = {}) {
  return {
    fetchPolicies: async () => door,
    loadPlans: () => [],
    runValidate: async (_g: string, rows: PolicyRow[], args?: string[]) => { seen.rows = rows.map((r) => r.name); seen.args = args; return stdout; },
    now: () => T0,
  };
}

type Kept = {
  domain: string; measuredAt: string | null; stale: boolean; checking: boolean; lastCheckFailed?: string;
  policies: Array<{ name: string; verdict: string }>;
};

beforeEach(() => clearPolicyCountsCache());

test('runs the domain lint over the policies that apply: global and scoped to it, nothing else', async () => {
  const seen: { rows?: string[]; args?: string[] } = {};
  const d = deps(RED, seen);
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const r = await fetchDomainConformance('principles', d);
  expect(r.status).toBe(200);
  expect(seen.args).toEqual(['--domain', 'principles']);
  expect((seen.rows ?? []).sort()).toEqual(['domain-has-owner', 'domain-referential-integrity', 'only-principles']);
  const body = r.body as { domain: string; measuredAt: string };
  expect(body.domain).toBe('principles');
  expect(body.measuredAt).toBe('2026-11-20T23:30:00.000Z');
});

test('NEGATIVE: with one check red, the fold shows that policy red with its count and violations', async () => {
  const d = deps(RED);
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', d)).body as {
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
  const d = deps(green);
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', d)).body as { policies: Array<{ verdict: string }> };
  expect(body.policies.length).toBe(3);
  expect(body.policies.every((p) => p.verdict === 'pass')).toBe(true);
});

test('NEGATIVE: a policy the run did not report is "could not run", never green', async () => {
  const d = deps('PASS domain-has-owner 0 <= 0');
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', d)).body as { policies: Array<{ name: string; verdict: string }> };
  expect(body.policies.find((p) => p.name === 'only-principles')!.verdict).toBe('broken');
});

test('NEGATIVE: a bad domain name is a 400, and starts no check', async () => {
  let runs = 0;
  const d = { ...deps(RED), runValidate: async () => { runs++; return RED; } };
  expect((await fetchDomainConformance('princ;x', d)).status).toBe(400);
  await domainCheckSettled('princ;x');
  expect(runs).toBe(0);
});

test('NEGATIVE: a run that measured nothing is not kept; a view a minute later starts the check again', async () => {
  let runs = 0;
  let clock = T0;
  const d = { ...deps(''), now: () => clock, runValidate: async () => { runs++; return ''; } };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  clock = T0 + 61 * 1000;
  const r = await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  expect((r.body as Kept).measuredAt).toBeNull();
  expect((r.body as Kept).policies).toEqual([]);
  expect(runs).toBe(2);
});

// ── #5044: what the page reads ────────────────────────────────────────────────────────────

test('#5044 the page\'s read answers before the lint has finished: nothing kept reads "not measured", check running', async () => {
  let release: (v: string) => void = () => {};
  const d = { ...deps(RED), runValidate: () => new Promise<string>((r) => { release = r; }) };
  // the lint is still running (it is released below): the read has already answered
  const first = (await fetchDomainConformance('principles', d)).body as Kept;
  expect(first.measuredAt).toBeNull();
  expect(first.policies).toEqual([]);
  expect(first.checking).toBe(true);
  release(RED);
  await domainCheckSettled('principles');
  const second = (await fetchDomainConformance('principles', d)).body as Kept;
  expect(second.measuredAt).toBe('2026-11-20T23:30:00.000Z');
  expect(second.policies.map((p) => p.verdict).sort()).toEqual(['fail', 'pass', 'pass']);
  expect([second.stale, second.checking]).toEqual([false, false]);
});

test('#5044 a kept result under ten minutes old is answered as it is and starts no check', async () => {
  let runs = 0;
  let clock = T0;
  const d = { ...deps(RED), now: () => clock, runValidate: async () => { runs++; return RED; } };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  clock = T0 + 9 * MIN;
  const body = (await fetchDomainConformance('principles', d)).body as Kept;
  await domainCheckSettled('principles');
  expect(runs).toBe(1);
  expect([body.stale, body.checking, body.measuredAt]).toEqual([false, false, '2026-11-20T23:30:00.000Z']);
});

test('#5044 NEGATIVE: a kept result past ten minutes is answered as STALE with its old time, never as current, and a fresh check starts behind it', async () => {
  let runs = 0;
  let clock = T0;
  const d = { ...deps(RED), now: () => clock, runValidate: async () => { runs++; return RED; } };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  clock = T0 + 11 * MIN;
  const body = (await fetchDomainConformance('principles', d)).body as Kept;
  expect(body.stale).toBe(true);
  expect(body.checking).toBe(true);
  expect(body.measuredAt).toBe('2026-11-20T23:30:00.000Z');   // the time it was measured, not the time it was asked
  await domainCheckSettled('principles');
  expect(runs).toBe(2);
  const after = (await fetchDomainConformance('principles', d)).body as Kept;
  expect([after.stale, after.measuredAt]).toEqual([false, new Date(T0 + 11 * MIN).toISOString()]);
});

test('#5044 NEGATIVE: many views while one check runs start one check, not one each', async () => {
  let runs = 0;
  let release: (v: string) => void = () => {};
  const d = { ...deps(RED), runValidate: () => { runs++; return new Promise<string>((r) => { release = r; }); } };
  for (let i = 0; i < 5; i++) await fetchDomainConformance('principles', d);
  await new Promise((r) => setImmediate(r));
  expect(runs).toBe(1);
  release(RED);
  await domainCheckSettled('principles');
});

test('#5044 NEGATIVE: a check that kept nothing is said so, with why; it is not shown as a result', async () => {
  const down = { ...deps(RED), fetchPolicies: async () => { throw new Error('ECONNREFUSED'); } };
  await fetchDomainConformance('principles', down);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', down)).body as Kept;
  await domainCheckSettled('principles');
  expect(body.measuredAt).toBeNull();
  expect(body.lastCheckFailed).toMatch(/policies door did not answer: ECONNREFUSED/);

  clearPolicyCountsCache();
  const empty = { ...deps(''), runValidate: async () => '' };
  await fetchDomainConformance('principles', empty);
  await domainCheckSettled('principles');
  const none = (await fetchDomainConformance('principles', empty)).body as Kept;
  await domainCheckSettled('principles');
  expect(none.measuredAt).toBeNull();
  expect(none.lastCheckFailed).toBe('the check ran and measured nothing');
});

test('#5044 NEGATIVE: for a minute after a check that kept nothing, a read starts no new check; after it, one', async () => {
  let runs = 0;
  let clock = T0;
  const d = { ...deps(''), now: () => clock, runValidate: async () => { runs++; return ''; } };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  // the page re-reads every 15 s while it waits: three more reads inside the minute
  for (const s of [15, 30, 45]) { clock = T0 + s * 1000; const b = (await fetchDomainConformance('principles', d)).body as Kept; expect(b.checking).toBe(false); expect(b.lastCheckFailed).toBe('the check ran and measured nothing'); }
  await domainCheckSettled('principles');
  expect(runs).toBe(1);
  clock = T0 + 61 * 1000;
  expect(((await fetchDomainConformance('principles', d)).body as Kept).checking).toBe(true);
  await domainCheckSettled('principles');
  expect(runs).toBe(2);
});

test('#5044 a domain no check applies to is kept as an empty answer, with its time, and runs no lint', async () => {
  let runs = 0;
  const d = { ...deps(RED), fetchPolicies: async () => [{ name: 'only-values', checkKind: 'command', checkSpec: 'x', baseline: '0', gate: 'code', appliesTo: 'chorus:values' }], runValidate: async () => { runs++; return RED; } };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', d)).body as Kept;
  expect([body.measuredAt, body.policies, body.checking, body.lastCheckFailed]).toEqual(['2026-11-20T23:30:00.000Z', [], false, undefined]);
  expect(runs).toBe(0);
});

test('#5044 a later check that succeeds clears the failure', async () => {
  let out = '';
  let clock = T0;
  const d = { ...deps(RED), now: () => clock, runValidate: async () => out };
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  out = RED;
  clock = T0 + 61 * 1000;   // past the minute in which a failed check is not started again
  await fetchDomainConformance('principles', d);
  await domainCheckSettled('principles');
  const body = (await fetchDomainConformance('principles', d)).body as Kept;
  expect(body.lastCheckFailed).toBeUndefined();
  expect(body.measuredAt).toBe(new Date(T0 + 61 * 1000).toISOString());
});
