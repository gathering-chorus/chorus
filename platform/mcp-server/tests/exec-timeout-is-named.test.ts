// @test-type: unit
// #4111 — a verb killed by execFile's own `timeout` must say so.
//
// 2026-09-06: two roles' pipelines died within an hour of each other and both
// of us read the failure as a verdict on our work.
//
//   #4111 (kade)  deploy-werk FAIL after 10m00.717s
//   #4102 (wren)  deploy-werk FAIL after 10m00.1s
//
// Ten minutes to the tenth of a second, twice, is a budget expiring. But
// execFile SIGTERMs the child on timeout, and a signal kill carries no numeric
// `code` — so `typeof e.code === 'number' ? e.code : 1` fell to exit 1, the
// reason regex found nothing, and the caller was told `reason=work-fail` with
// whatever stderr the child happened to have emitted last. Mine pasted a
// report-only SHACL warning from athena-deploy-model.sh, which reads exactly
// like a cause. I spent the morning chasing it.
//
// The detection already existed at server.ts (#3347, the cards exec) for the
// same class of starvation. This copies it onto the two exec sites that lacked
// it: executeServiceLifecycle (60s) and executeWerkVerb (600s).
//
// The 600s value itself is deliberately NOT touched here — that ceiling is
// Silas's, separately. This card only makes the failure legible.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { classifyExecFailure } from '../src/server';

// The two production patterns, verbatim from their call sites.
const LIFECYCLE_PATTERN = /reason=([a-z0-9-]+)/;
const VERB_PATTERN = /reason[=:]\s*([a-z0-9_-]+)/i;

// What execFile actually throws when its timeout fires. MEASURED, not assumed —
// the shape below is verbatim from running a real `sleep 10` under a 500ms
// budget on this box: `code` is null, not undefined, and `typeof null` is
// 'object', which is precisely why the old numeric check fell through.
//
//   {"killed":true,"signal":"SIGTERM","code":null,"typeofCode":"object"}
//
// A fixture invented from memory would have used undefined and still passed,
// for the wrong reason.
const timeoutKill = { killed: true, signal: 'SIGTERM' as const, code: null };

// The stderr my run really carried — report-only noise, not a cause.
const SHACL_NOISE =
  'athena-deploy-model: SHACL validator CRASHED — violations UNKNOWN, not 0 (#3731; report-only, deploy continues)';

// The logic exactly as it stood before this card, for the negative proof.
function oldClassify(err: { code?: number | null }, combined: string): string {
  const exitCode = typeof err.code === 'number' ? err.code : 1;
  const m = combined.match(VERB_PATTERN);
  return m ? m[1] : exitCode === 2 ? 'usage-error' : 'work-fail';
}

test('a verb killed by the exec timeout is classified as a timeout', () => {
  const got = classifyExecFailure(timeoutKill, SHACL_NOISE, VERB_PATTERN, 1);
  assert.equal(got.reason, 'timeout');
  assert.equal(got.timedOut, true);
});

test('NEGATIVE PROOF: the OLD logic calls that same kill work-fail', () => {
  // If this ever stops saying work-fail, the bug this card fixes is not the bug
  // described above and these tests are measuring the wrong thing.
  assert.equal(
    oldClassify(timeoutKill, SHACL_NOISE),
    'work-fail',
    'the pre-#4111 code must be shown to mis-report the kill, or there was nothing to fix',
  );
  assert.notEqual(classifyExecFailure(timeoutKill, SHACL_NOISE, VERB_PATTERN, 1).reason, 'work-fail');
});

test('killed without a signal (the plain execFile shape) is still a timeout', () => {
  const got = classifyExecFailure({ killed: true }, '', VERB_PATTERN, 1);
  assert.equal(got.reason, 'timeout');
});

test('a SIGTERM from elsewhere counts too — the child did not choose its exit', () => {
  const got = classifyExecFailure({ signal: 'SIGTERM' }, '', LIFECYCLE_PATTERN, 1);
  assert.equal(got.reason, 'timeout');
});

test('CONTROL: a real refusal keeps its own reason, not swallowed by the fix', () => {
  const got = classifyExecFailure({ code: 1 }, 'werk-deploy: reason=dirty-werk', VERB_PATTERN, 1);
  assert.equal(got.reason, 'dirty-werk');
  assert.equal(got.timedOut, false);
});

test('CONTROL: exit 2 is still usage-error, and exit 1 with no marker still work-fail', () => {
  assert.equal(classifyExecFailure({ code: 2 }, '', VERB_PATTERN, 2).reason, 'usage-error');
  assert.equal(classifyExecFailure({ code: 1 }, SHACL_NOISE, VERB_PATTERN, 1).reason, 'work-fail');
});

test('NEGATIVE PROOF: a genuine work-fail is NOT relabelled a timeout', () => {
  // The mirror of the first negative proof: the fix must be able to say
  // work-fail, or "timeout" would be as uninformative as "work-fail" was.
  const got = classifyExecFailure({ code: 1, killed: false }, SHACL_NOISE, VERB_PATTERN, 1);
  assert.equal(got.reason, 'work-fail');
  assert.equal(got.timedOut, false);
});

test('both exec sites route through the classifier — neither is left behind', () => {
  // A guard, not decoration: the whole defect was ONE exec site having the
  // detection (#3347) and the others not. If a site is reverted to the bare
  // `typeof e.code === 'number'` shape, this fails loudly.
  const src = readFileSync(join(__dirname, '..', 'src', 'server.ts'), 'utf8');
  const calls = (src.match(/classifyExecFailure\(/g) || []).length;
  assert.ok(calls >= 3, `expected the definition plus two call sites, found ${calls}`);

  // The two budgets this card converted are named, so a refusal can quote the
  // number it blew. A bare numeric literal at either site means a revert.
  assert.match(src, /timeout: LIFECYCLE_TIMEOUT_MS,/);
  assert.match(src, /timeout: VERB_TIMEOUT_MS,/);
});

test('a NEW exec timeout cannot be added without deciding about this class', () => {
  // Honesty about scope: this card converted the two sites that burned us, the
  // ones whose refusal carries a `reason=` a reader will believe. Two other
  // exec sites also carry timeouts (chorus_register_feedback 30s, flow-report
  // 120s). They are opaque on a kill rather than actively misleading, and they
  // are NOT converted here — recorded so they are visible rather than lost.
  //
  // This count is the tripwire. A ninth timeout site fails this test, and
  // whoever adds it has to say which bucket it is in.
  const src = readFileSync(join(__dirname, '..', 'src', 'server.ts'), 'utf8');
  const sites = (src.match(/timeout: [A-Za-z0-9_]+,?/g) || []).length;
  assert.equal(
    sites,
    8,
    'exec timeout sites changed — convert the new one to classifyExecFailure or add it to the unconverted list above',
  );
});

test('the thrown message names the budget so the reader need not time it by hand', () => {
  const got = classifyExecFailure(timeoutKill, SHACL_NOISE, VERB_PATTERN, 1, 600000);
  assert.match(got.detail, /600000ms/);
  assert.match(got.detail, /killed/i);
});
