/**
 * #3443 AC7 — werk run-state core: a transport drop must be a non-event.
 * Pins the contract a re-invoke relies on (idempotent attach, no double-act)
 * and that failures surface the child verb's real reason. mcp-server harness is
 * `tsx --test` (node:test), not jest.
 */
// @test-type: unit
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { announceRepeated, decideRunAction, extractFailureReason, parseExitSentinel, patchSuperseded, type WerkRun } from '../src/werk-run-state';

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

  test('PRESENTED but HEAD ADVANCED (new patch) -> start (#3538 re-demo the new commit)', () => {
    // A fix committed after a present must re-demo: the presented record is for an
    // old patch-id; HEAD moved past it, so the variant on record is stale.
    const r = run({ phase: 'presented', go: false });
    assert.deepEqual(decideRunAction(r, false, false, true), { kind: 'start' });
  });

  test('PRESENTED, SAME patch (content-identical rebase) -> attach (#3538 no needless re-run; sibling of #3461)', () => {
    // A churned sha with the same patch-id (a peer landed, our werk rebased, content
    // unchanged) must NOT re-demo — headChanged=false → attach the existing present.
    const r = run({ phase: 'presented', go: false });
    assert.deepEqual(decideRunAction(r, false, false, false), { kind: 'attach', run: r });
  });

  test('GO while still RUNNING -> typed refusal (#3678 AC2: was attach, which let a go float onto an unseen round)', () => {
    const r = run({ phase: 'running', go: false });
    assert.deepEqual(decideRunAction(r, true), { kind: 'refuse-go-running', run: r });
  });
});

describe('patchSuperseded — the #3538 comparison, hardened for empty records (#3638)', () => {
  // The #3421 stuck-present: a record persisted with patchId '' could never trip
  // headChanged, so a post-present fix commit attached to the stale present forever
  // (the only escape was hand-deleting the run record). An EMPTY recorded patch on a
  // known current patch must read as superseded — re-demo once, then the fresh record
  // carries a real key and polls attach normally.
  test('recorded EMPTY + current known -> superseded (legacy stuck record re-demos)', () => {
    assert.equal(patchSuperseded('', 'abc123'), true);
  });

  test('recorded undefined + current known -> superseded (same stuck class)', () => {
    assert.equal(patchSuperseded(undefined, 'abc123'), true);
  });

  test('same patch -> not superseded (poll attaches, no needless re-run)', () => {
    assert.equal(patchSuperseded('abc123', 'abc123'), false);
  });

  test('different patch -> superseded (#3538 re-demo the new commit)', () => {
    assert.equal(patchSuperseded('abc123', 'def456'), true);
  });

  test('current UNKNOWN (git hiccup at poll) -> not superseded (degrade to attach, never a spurious re-run)', () => {
    assert.equal(patchSuperseded('abc123', ''), false);
    assert.equal(patchSuperseded('', ''), false);
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

// ── #3678 — checking on a pipeline must never start new work ──

describe('decideRunAction — go while running refuses, typed (#3678 AC2)', () => {
  test('go on a RUNNING record → refuse-go-running (never silently queues onto an unseen round)', () => {
    const r = { runId: 'r', card: 1, role: 'kade', go: false, phase: 'running', startedAt: 't' } as WerkRun;
    assert.deepEqual(decideRunAction(r, true), { kind: 'refuse-go-running', run: r });
  });

  test('go on a PRESENTED record still starts the land (unchanged contract)', () => {
    const r = { runId: 'r', card: 1, role: 'kade', go: false, phase: 'presented', startedAt: 't' } as WerkRun;
    assert.deepEqual(decideRunAction(r, true), { kind: 'start' });
  });
});

describe('decideRunAction — explicit re-present is the only poll-side fresh-round trigger (#3678 AC3)', () => {
  test('represent on a presented record starts a fresh round', () => {
    const r = { runId: 'r', card: 1, role: 'kade', go: false, phase: 'presented', startedAt: 't' } as WerkRun;
    assert.deepEqual(decideRunAction(r, false, false, false, true), { kind: 'start' });
  });

  test('plain poll on a presented record attaches — N invokes, one round', () => {
    const r = { runId: 'r', card: 1, role: 'kade', go: false, phase: 'presented', startedAt: 't' } as WerkRun;
    for (let i = 0; i < 5; i++) {
      assert.deepEqual(decideRunAction(r, false, false, false, false), { kind: 'attach', run: r });
    }
  });
});

describe('announceRepeated — a repeated demo-ready announce is the SYSTEM\'s finding (#3678 AC4)', () => {
  test('same patch presented again inside the window → repeated', () => {
    assert.equal(
      announceRepeated(
        { presentedAt: '2026-07-23T16:35:00Z', patchId: 'p1' },
        '2026-07-23T16:48:00Z', 'p1', 30 * 60_000,
      ),
      true,
    );
  });

  test('different patch → not repeated (a real new round may announce)', () => {
    assert.equal(
      announceRepeated(
        { presentedAt: '2026-07-23T16:35:00Z', patchId: 'p1' },
        '2026-07-23T16:48:00Z', 'p2', 30 * 60_000,
      ),
      false,
    );
  });

  test('outside the window → not repeated', () => {
    assert.equal(
      announceRepeated(
        { presentedAt: '2026-07-23T10:00:00Z', patchId: 'p1' },
        '2026-07-23T16:48:00Z', 'p1', 30 * 60_000,
      ),
      false,
    );
  });

  test('no prior present → not repeated', () => {
    assert.equal(announceRepeated(null, '2026-07-23T16:48:00Z', 'p1', 30 * 60_000), false);
  });
});
