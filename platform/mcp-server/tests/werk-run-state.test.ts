/**
 * #3443 AC7 — werk run-state core: a transport drop must be a non-event.
 * Pins the contract a re-invoke relies on (idempotent attach, no double-act)
 * and that failures surface the child verb's real reason. mcp-server harness is
 * `tsx --test` (node:test), not jest.
 */
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { decideRunAction, extractFailureReason, type WerkRun } from '../src/werk-run-state';

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

  test('run PRESENTED, no GO -> attach (idempotent re-invoke returns the outcome)', () => {
    const r = run({ phase: 'presented' });
    assert.deepEqual(decideRunAction(r, false), { kind: 'attach', run: r });
  });

  test('run FAILED -> attach (report recorded failure, do not silently re-run)', () => {
    const r = run({ phase: 'failed', failureReason: 'announce-missing-this-round' });
    assert.deepEqual(decideRunAction(r, false), { kind: 'attach', run: r });
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
