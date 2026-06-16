/**
 * #3443 AC7 — werk run-state core: a transport drop must be a non-event.
 * Pins the contract a re-invoke relies on (idempotent attach, no double-act)
 * and that failures surface the child verb's real reason. mcp-server harness is
 * `tsx --test` (node:test), not jest.
 */
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { decideRunAction, extractFailureReason, parseExitSentinel, type WerkRun } from '../src/werk-run-state';

const run = (over: Partial<WerkRun> = {}): WerkRun => ({
  runId: 'r1', card: 3443, role: 'wren', go: false, phase: 'running',
  startedAt: '2026-06-15T23:40:00Z', ...over,
});

describe('decideRunAction — a re-invoke never double-acts', () => {
  test('no existing run -> start (first invocation)', () => {
    assert.deepEqual(decideRunAction(null, false), { kind: 'start' });
  });

  test('run still RUNNING -> attach, not a second act (drop-is-a-non-event)', () => {
    const r = run({ phase: 'running' });
    assert.deepEqual(decideRunAction(r, false), { kind: 'attach', run: r });
  });

  test('run RUNNING but STALE (dead pid / past TTL) -> start (kills the stale-running attach bug; #3458 + Wren #2)', () => {
    // belt+suspenders: if the durable terminal-phase write was lost (e.g. mcp
    // restart churned the run before act wrote its finish), a 'running' whose pid
    // is dead must not be attached-to forever — it is treated like 'failed'.
    const r = run({ phase: 'running', pid: 999999 });
    assert.deepEqual(decideRunAction(r, false, true), { kind: 'start' });
  });

  test('run RUNNING and LIVE -> attach (a genuinely-live run is never stranded)', () => {
    const r = run({ phase: 'running', pid: 4242 });
    assert.deepEqual(decideRunAction(r, false, false), { kind: 'attach', run: r });
  });

  test('GO while RUNNING-but-STALE -> start (a GO past a dead run is the retry, not an attach)', () => {
    const r = run({ phase: 'running', go: false, pid: 999999 });
    assert.deepEqual(decideRunAction(r, true, true), { kind: 'start' });
  });

  test('run PRESENTED, no GO -> attach (idempotent re-invoke returns the outcome)', () => {
    const r = run({ phase: 'presented' });
    assert.deepEqual(decideRunAction(r, false), { kind: 'attach', run: r });
  });

  test('run FAILED -> start (RETRYABLE; #3443 Kade catch — never strand on a transient failure)', () => {
    const r = run({ phase: 'failed', failureReason: 'merge: network hiccup' });
    assert.deepEqual(decideRunAction(r, false), { kind: 'start' });
  });

  test('GO on a FAILED run -> start (a GO must not be swallowed by a stale failure)', () => {
    const r = run({ phase: 'failed', go: false, failureReason: 'deploy-prod transient' });
    assert.deepEqual(decideRunAction(r, true), { kind: 'start' });
  });

  test('GO after a PRESENTED stop -> start (the land is the next legitimate phase)', () => {
    const r = run({ phase: 'presented', go: false });
    assert.deepEqual(decideRunAction(r, true), { kind: 'start' });
  });

  test('GO while still RUNNING -> attach (cannot land what is still presenting)', () => {
    const r = run({ phase: 'running', go: false });
    assert.deepEqual(decideRunAction(r, true), { kind: 'attach', run: r });
  });
});

describe('parseExitSentinel — the detached run wrote its own finish to the log (#3458)', () => {
  test('WERK_EXIT=0 -> 0 (act succeeded; the run is done, presented)', () => {
    assert.equal(parseExitSentinel('…build…\n[werk] done\nWERK_EXIT=0\n'), 0);
  });

  test('WERK_EXIT=1 -> 1 (act failed; the run is done, failed)', () => {
    assert.equal(parseExitSentinel('…\nFailure - Main merge\nWERK_EXIT=1\n'), 1);
  });

  test('no sentinel -> null (act still running, or crashed before writing it)', () => {
    assert.equal(parseExitSentinel('…build still going…\n'), null);
  });

  test('takes the LAST sentinel if the log was reused', () => {
    assert.equal(parseExitSentinel('WERK_EXIT=1\n…\nWERK_EXIT=0\n'), 0);
  });
});

describe('extractFailureReason — surface the child reason, never just step=X', () => {
  test('structured {"reason":"..."} wins', () => {
    assert.equal(extractFailureReason('{"ok":false,"reason":"no-open-pr"}', '', 'merge'), 'no-open-pr');
  });

  test('reason=<token> in the error line', () => {
    assert.equal(
      extractFailureReason('', 'werk-merge: refused reason=round-expired card=3443', 'merge'),
      'round-expired',
    );
  });

  test('falls back to the last stderr line when no reason field', () => {
    assert.equal(
      extractFailureReason('', 'building...\nwerk-merge: PR #626 has merge conflicts', 'merge'),
      'werk-merge: PR #626 has merge conflicts',
    );
  });

  test('falls back to step name only when nothing richer exists', () => {
    assert.equal(extractFailureReason('', '', 'deploy'), 'step=deploy (no child reason surfaced)');
  });
});
