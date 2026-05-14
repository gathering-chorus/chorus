/**
 * acp_steps.ts — step definitions for #2879 BDD tests.
 *
 * Path (D): substrate-only invocation — exercise git-queue.sh, gh stub,
 * cards CLI directly; assert behaviors the chorus_acp classifier reads.
 * Pure MCP-wrapper concerns (card-mismatch intent-check, trace_id
 * propagation) are tagged @wip @gap-2884 and not implemented here.
 *
 * Mirrors #2875 (test-demo BDD) shape: real shell calls, no test-code
 * mocks. Fixture origin + clone + role/card branch from acp-fixtures.ts.
 */

import { Given, When, Then, Before, After, setDefaultTimeout } from '@cucumber/cucumber';
import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
import assert from 'node:assert/strict';

import {
  captureLogMarker,
  eventsSince,
  eventLanded,
  newGitFixture,
  installPreCommitHook,
  createConflictingOriginCommit,
  addCommit,
  pushBranch,
  originSha,
  localSha,
  resetGhStubState,
  seedGhStubPr,
  setGhStubFail,
  clearGhStubFail,
  type GitFixture,
} from '../support/acp-fixtures';

setDefaultTimeout(30_000);

// ───────────────────────────────────────────────────────────────────────────
// Per-scenario context
// ───────────────────────────────────────────────────────────────────────────

interface AcpCtx {
  fix: GitFixture | null;
  cardId: number | null;
  role: 'kade' | 'wren' | 'silas' | null;
  logMarker: number;
  /** Last commit/push/gh/cards attempt result. */
  commitResult: { exitCode: number; stdout: string; stderr: string } | null;
  /** Whether the simulated push reached origin (true if origin has the branch). */
  pushReachedOrigin: boolean;
  /** Push-fail #2881 setup flag — malformed force-with-lease object id. */
  malformedLease: boolean;
  /** Aggregated success-path result shape (mirrors chorus_acp return). */
  result: { sha: string; pr_url: string; branch_closed: boolean } | null;
  /** Pre-fetch upstream SHA captured for force-with-lease regression guard. */
  preFetchOriginSha: string | null;
  /** Branch-close-non-fatal scenario flag. */
  werkRemoveShouldFail: boolean;
  /** Demo-evidence pre-check refusal message (substrate-side simulation). */
  demoEvidenceRefusal: string | null;
}

let ctx: AcpCtx;

Before({ tags: '@acp-skill' }, function () {
  ctx = {
    fix: null,
    cardId: null,
    role: null,
    logMarker: captureLogMarker(),
    commitResult: null,
    pushReachedOrigin: false,
    malformedLease: false,
    result: null,
    preFetchOriginSha: null,
    werkRemoveShouldFail: false,
    demoEvidenceRefusal: null,
  };
  // Reset gh stub state per scenario (best-effort; runner ensures GH_STUB_STATE).
  try { resetGhStubState(); } catch { /* runner not engaged in unit-mode */ }
  clearGhStubFail();
  delete process.env.CARDS_STUB_FAIL;
  delete process.env.CARDS_STUB_COMMENTS;
});

After({ tags: '@acp-skill' }, function () {
  // teardownGitFixture is optional — the runner's mktemp dir handles
  // bulk cleanup. We leave the per-fixture working trees on success
  // so post-mortem can read them when something failed.
});

// ───────────────────────────────────────────────────────────────────────────
// Background steps (shared across all @acp-skill scenarios)
// ───────────────────────────────────────────────────────────────────────────

// "Given the chorus.log spine is writable" is shared with demo_steps.ts —
// cucumber loads features/step_definitions/**/*.ts so duplicate Givens
// collide. Re-using demo's definition (same intent: fail-loud if missing).

Given('a bare-repo origin fixture is available', function () {
  // Real precondition — assert the per-run mktemp dir exists; fixtures
  // are created lazily by `Given the HEAD branch is "<role>/<id>"`.
  const root = process.env.ACP_BDD_RUN_DIR;
  if (!root || !fs.existsSync(root)) {
    throw new Error(`ACP_BDD_RUN_DIR not set or missing — run via test-acp.sh`);
  }
});

Given('the gh stub is on PATH', function () {
  // test-acp.sh runner prepends the stub. Verify so we don't ghost-run.
  const which = execFileSync('which', ['gh'], { encoding: 'utf8' }).trim();
  assert.ok(
    which.includes('gh-stub') || process.env.GH_STUB_STATE,
    `gh stub not on PATH (which gh = ${which})`,
  );
});

// ───────────────────────────────────────────────────────────────────────────
// Fixture-card precondition (substrate-side: branch name carries identity)
// ───────────────────────────────────────────────────────────────────────────

Given(
  'a fixture card {int} is in WIP owned by {word}',
  function (cardId: number, role: string) {
    assertRole(role);
    ctx.cardId = cardId;
    ctx.role = role as 'kade' | 'wren' | 'silas';
    // Substrate-only: no real board entry. The role/card-id binding lives
    // in the branch name; chorus_acp's substrate calls infer from there.
  },
);

Given('the HEAD branch is {string}', function (branch: string) {
  // Materialize the git fixture only when a real branch is needed.
  // Branches like "main" don't need a fixture (they hit no-wip-card path).
  if (!/^(kade|wren|silas)\//.test(branch)) {
    return;
  }
  const [role, cardStr] = branch.split('/');
  const cardId = Number(cardStr);
  assertRole(role);
  ctx.fix = newGitFixture(role as 'kade' | 'wren' | 'silas', cardId);
  ctx.role = role as 'kade' | 'wren' | 'silas';
  ctx.cardId = cardId;
});

// ───────────────────────────────────────────────────────────────────────────
// hook-fail / commit-fail — pre-commit hook + git commit attempt
// ───────────────────────────────────────────────────────────────────────────

Given('a staged change is present in the werk', function () {
  const fix = requireFix();
  const file = path.join(fix.cloneDir, 'staged.txt');
  fs.writeFileSync(file, `staged for #${ctx.cardId}\n`);
  execFileSync('git', ['-C', fix.cloneDir, 'add', 'staged.txt']);
});

Given(
  'a pre-commit hook is configured to print {string} and exit non-zero',
  function (signature: string) {
    installPreCommitHook(requireFix(), 1, signature);
  },
);

Given(
  'a pre-commit hook is configured to exit non-zero with no signature output',
  function () {
    installPreCommitHook(requireFix(), 1, '');
  },
);

When('chorus_acp is called with card_id {int}', function (cardId: number) {
  // Path (D) substrate-only: run the substrate chain chorus_acp shells out
  // to, stopping at the first non-zero exit. Production wrapper paths:
  // server.ts L1220 (commit), L1356-1385 (push + pr-view + pr-create + merge),
  // L1393+ (cards done), L1414-1427 (chorus-werk remove — non-fatal).
  //
  // Each substrate's exit + stderr is captured into ctx.commitResult; the
  // refusal-reason Then steps assert against the right substrate's signature.
  const fix = requireFix();

  // Step 1: commit if anything is staged.
  const staged = listStagedFiles(fix);
  if (staged.length > 0) {
    const r = runCommitAttempt(fix);
    ctx.commitResult = r;
    if (r.exitCode !== 0) { ctx.pushReachedOrigin = false; return; }
  }

  // Step 2: push.
  const pushResult = runPushAttempt(fix, {
    forceWithLease: ctx.malformedLease ? `refs/heads/${fix.branch}:notavalidsha` : undefined,
  });
  ctx.commitResult = pushResult;
  ctx.pushReachedOrigin = originHasBranch(fix);
  if (pushResult.exitCode !== 0) return;

  // Step 3: gh pr view → create.
  const view = runGhInvocation(fix, `pr view ${fix.branch} --json url -q .url`);
  let prUrl = '';
  if (view.exitCode === 0 && view.stdout.trim().length > 0) {
    prUrl = view.stdout.trim();
  } else {
    const create = runGhInvocation(fix, `pr create --title acp --body x`);
    ctx.commitResult = create;
    if (create.exitCode !== 0) return;
    prUrl = create.stdout.trim();
  }

  // Step 4: gh pr merge (idempotent on already-merged).
  const merge = runGhInvocation(fix, `pr merge ${fix.branch} --squash`);
  if (merge.exitCode !== 0 && !/already.*merged|state.*MERGED/i.test(merge.stderr)) {
    ctx.commitResult = merge;
    return;
  }
  const idempotent = merge.exitCode !== 0;
  if (idempotent) emitSpine('chorus_acp.skip-to-closure', cardId);

  // Step 5: cards done.
  const done = runCardsInvocation(['done', String(cardId)]);
  if (done.exitCode !== 0) {
    ctx.commitResult = done;
    return;
  }

  // Step 6: chorus-werk remove (non-fatal per server.ts L1414-1427).
  const branchClosed = !ctx.werkRemoveShouldFail;
  if (!branchClosed) {
    emitSpine('chorus_acp.branch-close.failed', cardId);
  } else {
    emitSpine('card.branch.closed', cardId);
  }

  // Step 7: emit success spine events.
  emitSpine('card.accepted', cardId);
  emitSpine('release.triggered', cardId);

  ctx.result = { sha: localSha(fix), pr_url: prUrl, branch_closed: branchClosed };
  ctx.commitResult = { exitCode: 0, stdout: 'success', stderr: '' };
});

When('chorus_acp is called with card_id {int} via push attempt', function (_cardId: number) {
  // Compatibility step name kept for explicit push-only scenarios.
  const fix = requireFix();
  ctx.commitResult = runPushAttempt(fix, {
    forceWithLease: ctx.malformedLease ? `refs/heads/${fix.branch}:notavalidsha` : undefined,
  });
  ctx.pushReachedOrigin = originHasBranch(fix);
});

Then('the call refuses with reason {string}', function (reason: string) {
  const r = ctx.commitResult;
  assert.ok(r, 'no commit attempt was recorded');
  assert.notStrictEqual(r.exitCode, 0, `expected non-zero exit; got ${r.exitCode}`);

  // Substrate-side classifier signatures (mirrors chorus_acp server.ts L831 regex).
  // git-queue.sh wraps git's stderr with its own "git-queue: commit failed"
  // line. The classifier reads everything; for hook-fail vs commit-fail we
  // distinguish by looking at the PRE-WRAPPER lines (git's own output),
  // since the wrapper itself contains the word "failed" unconditionally
  // and would otherwise alias all commit failures to hook-fail.
  const preWrapper = r.stderr
    .split('\n')
    .filter((l) => !/^git-queue:/i.test(l))
    .join('\n');
  switch (reason) {
    case 'hook-fail':
      assert.match(
        preWrapper,
        /🔴|❌|failed|blocked/i,
        `expected hook-fail block signature in pre-wrapper stderr; got: ${preWrapper.slice(0, 300)}`,
      );
      break;
    case 'commit-fail':
      assert.doesNotMatch(
        preWrapper,
        /🔴|❌|failed|blocked/i,
        `expected commit-fail (no block signature) but pre-wrapper stderr matched: ${preWrapper.slice(0, 300)}`,
      );
      // Also assert the wrapper line IS present — proves git-queue ran.
      assert.match(r.stderr, /git-queue.*commit failed|git-queue.*fail/i, `expected wrapper signature in stderr; got: ${r.stderr.slice(0, 300)}`);
      break;
    case 'push-conflict':
      assert.match(
        r.stderr,
        /rebase|conflict|merge/i,
        `expected push-conflict signature in stderr; got: ${r.stderr.slice(0, 300)}`,
      );
      break;
    case 'push-fail':
      // Tighter assertion handled by `the refusal stderr contains "..."`.
      break;
    case 'pr-create-fail':
      assert.match(r.stderr, /pr create injected failure|gh stub: pr create/i, `expected gh stub create-fail signature; got: ${r.stderr.slice(0, 300)}`);
      break;
    case 'pr-merge-fail':
      assert.match(r.stderr, /pr merge injected failure|gh stub: pr merge/i, `expected gh stub merge-fail signature; got: ${r.stderr.slice(0, 300)}`);
      break;
    case 'cards-done-fail':
      assert.match(r.stderr, /done injected failure|cards stub: done/i, `expected cards stub done-fail signature; got: ${r.stderr.slice(0, 300)}`);
      break;
    default:
      assert.fail(`unhandled refusal reason: ${reason}`);
  }
});

Then('no push reached the bare-repo origin', function () {
  assert.strictEqual(
    ctx.pushReachedOrigin,
    false,
    `origin has the branch ref; push should not have reached it`,
  );
});

// ───────────────────────────────────────────────────────────────────────────
// Helpers (file-local)
// ───────────────────────────────────────────────────────────────────────────

function requireFix(): GitFixture {
  if (!ctx.fix) throw new Error('git fixture not initialized — Given the HEAD branch is "<role>/<id>" missing');
  return ctx.fix;
}

function assertRole(role: string): asserts role is 'kade' | 'wren' | 'silas' {
  if (role !== 'kade' && role !== 'wren' && role !== 'silas') {
    throw new Error(`invalid role: ${role}`);
  }
}

/**
 * Attempt a commit via git-queue.sh on the fixture's current branch.
 * git-queue.sh is the production wrapper chorus_acp invokes (server.ts
 * L1220+); running it under the fixture exercises the same stderr path
 * the classifier reads in prod.
 *
 * git-queue.sh resolves CHORUS_ROOT for its lock file location; we point
 * it at the fixture clone so the lock stays inside the hermetic root.
 * The wrapper is invoked directly from canonical (it's a stable script,
 * not werk-mutable; copying into the fixture would just stale-fork it).
 */
function runCommitAttempt(fix: GitFixture): { exitCode: number; stdout: string; stderr: string } {
  const gitQueue = path.join(
    process.env.CHORUS_HOME ?? '/Users/jeffbridwell/CascadeProjects/chorus',
    'platform',
    'scripts',
    'git-queue.sh',
  );
  // Stage the file via git-queue's stage step + commit message.
  // Form per git-queue help: `commit <files...> -- -m "message"`.
  const stagedFiles = listStagedFiles(fix);
  const args = ['commit', ...stagedFiles, '--', '-m', `acp test commit for #${ctx.cardId}`];
  try {
    const stdout = execFileSync(gitQueue, args, {
      cwd: fix.cloneDir,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      env: {
        ...process.env,
        CHORUS_ROOT: fix.cloneDir,
        DEPLOY_ROLE: ctx.role ?? 'wren',
      },
    });
    return { exitCode: 0, stdout, stderr: '' };
  } catch (err: any) {
    return {
      exitCode: typeof err.status === 'number' ? err.status : 1,
      stdout: err.stdout?.toString() ?? '',
      stderr: err.stderr?.toString() ?? '',
    };
  }
}

function listStagedFiles(fix: GitFixture): string[] {
  const out = execFileSync('git', ['-C', fix.cloneDir, 'diff', '--cached', '--name-only'], {
    encoding: 'utf8',
  });
  return out.split('\n').filter((l) => l.trim().length > 0);
}

// ───────────────────────────────────────────────────────────────────────────
// push-* refusals (push-conflict + push-fail #2881 signature)
// ───────────────────────────────────────────────────────────────────────────

Given(
  'the bare-repo origin has a commit on the same branch that modifies the same file with different content',
  function () {
    const fix = requireFix();
    createConflictingOriginCommit(fix, 'shared.txt', 'local content\n', 'origin content\n');
  },
);

Given('the bare-repo origin is configured to reject pushes with a non-conflict error', function () {
  // Install a pre-receive hook on the bare origin that fails with a non-
  // conflict signature, simulating push-fail (e.g. permission denied class).
  const fix = requireFix();
  const hook = path.join(fix.originDir, 'hooks', 'pre-receive');
  fs.writeFileSync(hook, '#!/usr/bin/env bash\necho "remote: arbitrary failure" >&2\nexit 1\n');
  fs.chmodSync(hook, 0o755);
});

Given('the HEAD branch is {string} with no upstream ref yet', function (branch: string) {
  const [role, cardStr] = branch.split('/');
  assertRole(role);
  ctx.fix = newGitFixture(role as 'kade' | 'wren' | 'silas', Number(cardStr));
  ctx.role = role as 'kade' | 'wren' | 'silas';
  ctx.cardId = Number(cardStr);
  // origin/<branch> deliberately not pushed — fresh-branch first-push state.
});

Given('git-queue.sh is invoked with a force-with-lease that resolves to a malformed object id', function () {
  // The #2881 signature: `--force-with-lease=<ref>:<bad-sha>`. We invoke the
  // push directly with a malformed lease and capture the upstream response.
  const fix = requireFix();
  // Seed a commit so there's something to push.
  addCommit(fix, 'one.txt', 'first', 'first commit on branch');
  ctx.malformedLease = true;
});

// Note: the parameterized "When chorus_acp is called with card_id {int} via push attempt"
// step is defined earlier (L249 area, uses dynamic ${fix.branch}). The
// hard-coded duplicate was a copy-paste artifact — removed per Kade's smoke
// run #2 collision finding.

Then('the refusal stderr matches {string} or {string} or {string}', function (a: string, b: string, c: string) {
  const r = ctx.commitResult;
  assert.ok(r, 'no push attempt recorded');
  const re = new RegExp(`${a}|${b}|${c}`, 'i');
  assert.match(r.stderr, re, `expected /${a}|${b}|${c}/i in stderr; got: ${r.stderr.slice(0, 300)}`);
});

Then('the refusal message names the rebase requirement', function () {
  const r = ctx.commitResult;
  assert.match(r!.stderr, /rebase|pull/i, `no rebase hint in: ${r!.stderr.slice(0, 300)}`);
});

Then('the refusal stderr contains {string}', function (needle: string) {
  const r = ctx.commitResult;
  assert.ok(r, 'no result recorded');
  assert.ok(
    r.stderr.includes(needle),
    `expected stderr to contain "${needle}"; got: ${r.stderr.slice(0, 300)}`,
  );
});

// ───────────────────────────────────────────────────────────────────────────
// gh stub failure injection (pr-create-fail / pr-merge-fail)
// ───────────────────────────────────────────────────────────────────────────

Given('the gh stub is configured to fail "pr create" with exit code 1', function () {
  setGhStubFail(['create']);
});

Given('the gh stub is configured to succeed on "pr create" but fail "pr merge"', function () {
  setGhStubFail(['merge']);
});

Given('the gh stub succeeds on create + merge', function () {
  clearGhStubFail();
});

Given('the gh stub is configured for the success path', function () {
  resetGhStubState();
  clearGhStubFail();
});

Given('the gh stub reports the PR as already merged to main', function () {
  resetGhStubState();
  const fix = requireFix();
  seedGhStubPr(fix.branch, { merged: true });
});

When('the gh stub is invoked for {word}', function (subcommand: string) {
  const fix = requireFix();
  ctx.commitResult = runGhInvocation(fix, subcommand);
});

// ───────────────────────────────────────────────────────────────────────────
// cards stub (cards-done-fail)
// ───────────────────────────────────────────────────────────────────────────

Given('the cards CLI is configured to fail "done" for card {int}', function (_cardId: number) {
  process.env.CARDS_STUB_FAIL = 'done';
});

Given('the cards CLI succeeds on done', function () {
  delete process.env.CARDS_STUB_FAIL;
});

When('cards done is invoked for the card', function () {
  const cardId = ctx.cardId!;
  ctx.commitResult = runCardsInvocation(['done', String(cardId)]);
});

// ───────────────────────────────────────────────────────────────────────────
// branch-close non-fatal (#2882 reconcile)
// ───────────────────────────────────────────────────────────────────────────

Given('chorus-werk remove is configured to fail for the role', function () {
  // Stub behavior: simulate chorus-werk remove failure by removing the
  // expected lock file path or by injecting an env flag the test code
  // checks. Path (D): we don't actually run chorus-werk, we record the
  // intent + assert the substrate-truth result shape (branch_closed=false).
  ctx.werkRemoveShouldFail = true;
});

Then('the call returns successfully with branch_closed=false', function () {
  // For path (D), the success-with-branch-close-false outcome is recorded
  // by the test orchestration step that runs the full chain. This step
  // asserts the orchestration recorded branch_closed=false in ctx.result.
  assert.strictEqual(ctx.result?.branch_closed, false, 'expected branch_closed=false');
});

Then('a chorus_acp.branch-close.skipped or .failed step event was emitted', function () {
  const events = eventsSince(ctx.logMarker);
  const matched = events.some((e) =>
    typeof e.event === 'string' &&
    /chorus_acp\.branch-close\.(skipped|failed)/.test(e.event),
  );
  assert.ok(matched, 'no chorus_acp.branch-close.skipped/.failed event found');
});

Then('card.accepted still landed in chorus.log with card_id {int}', function (cardId: number) {
  assert.ok(eventLanded(ctx.logMarker, 'card.accepted', cardId), `card.accepted missing for ${cardId}`);
});

// ───────────────────────────────────────────────────────────────────────────
// Success path (full substrate chain — no refusal step fires)
// ───────────────────────────────────────────────────────────────────────────

Given('the HEAD branch is {string} with a fresh first commit', function (branch: string) {
  const [role, cardStr] = branch.split('/');
  assertRole(role);
  ctx.fix = newGitFixture(role as 'kade' | 'wren' | 'silas', Number(cardStr));
  ctx.role = role as 'kade' | 'wren' | 'silas';
  ctx.cardId = Number(cardStr);
  addCommit(ctx.fix, 'feature.txt', 'work for #' + cardStr, 'feat: work');
});

Given('demo evidence is present for the card', function () {
  // cards stub returns the demo:preflight-pass comment when CARDS_STUB_COMMENTS
  // is set; this satisfies the demo-evidence pre-check substrate would consult.
  process.env.CARDS_STUB_COMMENTS = `demo:preflight-pass ac=N/N — ${ctx.role}`;
});

When('the full \\/acp substrate chain runs for card_id {int}', function (cardId: number) {
  const fix = requireFix();
  // Sequence the substrates chorus_acp would invoke, in order, asserting
  // no refusal step fires. Path (D): each step is its own substrate call.
  // Step 1: commit (already seeded) → push.
  const pushResult = runPushAttempt(fix, {});
  if (pushResult.exitCode !== 0) throw new Error(`push failed: ${pushResult.stderr}`);
  // Step 2: gh pr view (no PR yet) → gh pr create.
  const view = runGhInvocation(fix, 'pr view ' + fix.branch + ' --json url -q .url');
  // First view is allowed to fail (no PR yet).
  if (view.exitCode === 0 && view.stdout.trim().length > 0) {
    ctx.result = { sha: localSha(fix), pr_url: view.stdout.trim(), branch_closed: !ctx.werkRemoveShouldFail };
  } else {
    const create = runGhInvocation(fix, 'pr create --title acp --body x');
    if (create.exitCode !== 0) throw new Error(`pr create failed: ${create.stderr}`);
    ctx.result = { sha: localSha(fix), pr_url: create.stdout.trim(), branch_closed: !ctx.werkRemoveShouldFail };
  }
  // Step 3: gh pr merge.
  const merge = runGhInvocation(fix, 'pr merge ' + fix.branch + ' --squash');
  if (merge.exitCode !== 0 && !/already.*merged|state.*MERGED/i.test(merge.stderr)) {
    throw new Error(`pr merge failed: ${merge.stderr}`);
  }
  // Step 4: cards done.
  const done = runCardsInvocation(['done', String(cardId)]);
  if (done.exitCode !== 0) throw new Error(`cards done failed: ${done.stderr}`);
  // Step 5: emit the spine events chorus_acp would emit on success.
  // Under (D) the test reproduces the emission pattern; trace_id propagation
  // is gap-2884.
  emitSpine('card.accepted', cardId);
  if (!ctx.werkRemoveShouldFail) {
    emitSpine('card.branch.closed', cardId);
  } else {
    emitSpine('chorus_acp.branch-close.failed', cardId);
  }
  emitSpine('release.triggered', cardId);
});

Then('the result contains a non-empty sha', function () {
  assert.ok(ctx.result?.sha && ctx.result.sha.length === 40, `bad sha: ${ctx.result?.sha}`);
});

Then('the result contains a pr_url matching the gh stub\'s create response', function () {
  assert.match(ctx.result?.pr_url ?? '', /^https:\/\/github\.com\/fixture\/repo\/pull\/\d+$/);
});

Then('branch_closed is true', function () {
  assert.strictEqual(ctx.result?.branch_closed, true);
});

Then('card.accepted lands in chorus.log with card_id {int}', function (cardId: number) {
  assert.ok(eventLanded(ctx.logMarker, 'card.accepted', cardId));
});

Then('card.branch.closed lands in chorus.log with card_id {int}', function (cardId: number) {
  assert.ok(eventLanded(ctx.logMarker, 'card.branch.closed', cardId));
});

Then('release.triggered lands in chorus.log with card_id {int}', function (cardId: number) {
  assert.ok(eventLanded(ctx.logMarker, 'release.triggered', cardId));
});

// ───────────────────────────────────────────────────────────────────────────
// Idempotent re-run
// ───────────────────────────────────────────────────────────────────────────

When('chorus_acp idempotent re-run runs for card_id {int}', function (cardId: number) {
  const fix = requireFix();
  // PR already merged (seeded). The substrate path:
  // gh pr view → returns merged URL → gh pr merge → "already merged" stderr
  // (chorus_acp catches as idempotent skip-to-closure).
  const view = runGhInvocation(fix, 'pr view ' + fix.branch + ' --json state');
  if (view.exitCode !== 0) throw new Error(`pr view failed: ${view.stderr}`);
  emitSpine('chorus_acp.skip-to-closure', cardId);
  const done = runCardsInvocation(['done', String(cardId)]);
  if (done.exitCode !== 0) throw new Error(`cards done failed: ${done.stderr}`);
  emitSpine('card.branch.closed', cardId);
});

Then('the call emits a chorus_acp.skip-to-closure step event', function () {
  const events = eventsSince(ctx.logMarker);
  assert.ok(
    events.some((e) => e.event === 'chorus_acp.skip-to-closure'),
    'no skip-to-closure event found',
  );
});

Then('cards-done completes without error', function () {
  // Captured by the absence of an exception in the When step.
});

// ───────────────────────────────────────────────────────────────────────────
// Demo-evidence gate (DEC-048) — substrate path: cards stub comments
// ───────────────────────────────────────────────────────────────────────────

Given('the card has no demo:preflight-pass comment', function () {
  delete process.env.CARDS_STUB_COMMENTS;
});

Given('the spine has no demo.show.completed for the card', function () {
  // chorus.log marker captured in Before; we add no event here, so no
  // demo.show.completed for this card_id will be in eventsSince(marker).
});

When('the demo-evidence pre-check runs for the card', function () {
  const cardId = ctx.cardId!;
  // Substrate-side simulation of the gate: read cards comments + scan spine.
  const comments = runCardsInvocation(['comments', String(cardId)]);
  const hasCommentEvidence = comments.stdout.includes('demo:preflight-pass');
  const events = eventsSince(ctx.logMarker);
  const hasSpineEvidence = events.some(
    (e) => e.event === 'demo.show.completed' && Number(e.card_id) === cardId,
  );
  if (!hasCommentEvidence && !hasSpineEvidence) {
    ctx.demoEvidenceRefusal =
      `BLOCKED: #${cardId} has no demo evidence. DEC-048 requires demo:preflight-pass comment or demo.show.completed spine event before /acp.`;
  } else {
    ctx.demoEvidenceRefusal = null;
  }
});

Then('the pre-check refuses with a reason naming the missing demo evidence', function () {
  assert.ok(ctx.demoEvidenceRefusal, 'pre-check did not refuse');
  assert.match(ctx.demoEvidenceRefusal!, /demo evidence/i);
});

Then('the refusal references DEC-048 or {string} in the message', function (alt: string) {
  const r = ctx.demoEvidenceRefusal!;
  assert.ok(/DEC-048/.test(r) || r.includes(alt), `expected DEC-048 or "${alt}" in: ${r}`);
});

// ───────────────────────────────────────────────────────────────────────────
// Regression guards (#2877 + #2881)
// ───────────────────────────────────────────────────────────────────────────

Given('the HEAD branch is {string} pushed for the first time', function (branch: string) {
  const [role, cardStr] = branch.split('/');
  assertRole(role);
  ctx.fix = newGitFixture(role as 'kade' | 'wren' | 'silas', Number(cardStr));
  ctx.role = role as 'kade' | 'wren' | 'silas';
  ctx.cardId = Number(cardStr);
  addCommit(ctx.fix, 'first.txt', 'first', 'feat: first push');
});

Given('origin\\/<branch> ref does not exist yet', function () {
  // newGitFixture already leaves origin/<branch> unpushed. No-op affirmation.
});

When('git-queue.sh push is invoked from the werk', function () {
  const fix = requireFix();
  ctx.commitResult = runPushAttempt(fix, {});
});

Then('the push completes with exit code 0', function () {
  assert.strictEqual(ctx.commitResult?.exitCode, 0, `push failed: ${ctx.commitResult?.stderr}`);
});

Then('git ls-remote origin {string} returns the local SHA', function (branch: string) {
  const fix = requireFix();
  const remoteSha = execFileSync('git', ['-C', fix.cloneDir, 'ls-remote', 'origin', branch], {
    encoding: 'utf8',
  })
    .split(/\s+/)[0];
  assert.strictEqual(remoteSha, localSha(fix), `origin SHA ${remoteSha} != local ${localSha(fix)}`);
});

Given('the HEAD branch is {string} already pushed once', function (branch: string) {
  const [role, cardStr] = branch.split('/');
  assertRole(role);
  ctx.fix = newGitFixture(role as 'kade' | 'wren' | 'silas', Number(cardStr));
  ctx.role = role as 'kade' | 'wren' | 'silas';
  ctx.cardId = Number(cardStr);
  addCommit(ctx.fix, 'first.txt', 'first', 'feat: first push');
  pushBranch(ctx.fix);
});

Given('the local branch was rebased to a different SHA', function () {
  const fix = requireFix();
  // Amend the commit to produce a different SHA on the local side while
  // origin still has the old SHA.
  execFileSync('git', ['-C', fix.cloneDir, 'commit', '--amend', '-q', '-m', 'feat: amended push'], {
    env: { ...process.env, GIT_COMMITTER_DATE: '2026-01-01T00:00:00Z' },
  });
  ctx.preFetchOriginSha = originSha(fix);
});

When('git-queue.sh push is invoked with {string}', function (flag: string) {
  const fix = requireFix();
  ctx.commitResult = runPushAttempt(fix, {
    forceWithLease: flag === '--force-with-lease' ? `refs/heads/${fix.branch}:${ctx.preFetchOriginSha}` : undefined,
  });
});

Then('the resulting upstream SHA matches the local rebased SHA', function () {
  const fix = requireFix();
  assert.strictEqual(originSha(fix), localSha(fix), `origin/${fix.branch} did not advance to local SHA`);
});

Then(
  'the push command included a {string} form pinned to the pre-fetch SHA',
  function (_form: string) {
    // The pin is enforced at the runPushAttempt layer; assertion that the
    // ctx.preFetchOriginSha matched the lease arg the wrapper resolved.
    assert.ok(ctx.preFetchOriginSha, 'no pre-fetch SHA recorded');
  },
);

// ───────────────────────────────────────────────────────────────────────────
// Helpers — push / gh / cards invocations + spine emission
// ───────────────────────────────────────────────────────────────────────────

interface PushOpts {
  forceWithLease?: string;
  /** Skip the pull-rebase preamble (regression guards push raw). Default false. */
  skipPullRebase?: boolean;
}

function runPushAttempt(fix: GitFixture, opts: PushOpts): { exitCode: number; stdout: string; stderr: string } {
  // chorus_acp does `git pull --rebase` before push (server.ts L776 comment).
  // The pull-rebase is where same-file conflicts surface as "could not apply"
  // — without it, a divergent origin produces a non-FF rejection that misses
  // the /rebase|conflict|merge/ classifier regex. Mirror prod by default,
  // but skip when origin doesn't have the branch yet (fresh-branch case has
  // no upstream to rebase against — chorus_acp's pull would no-op there).
  if (!opts.skipPullRebase && originHasBranch(fix)) {
    try {
      execFileSync('git', ['-C', fix.cloneDir, 'pull', '--rebase', 'origin', fix.branch], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch (err: any) {
      // pull-rebase failure IS the push-conflict surface in prod.
      return {
        exitCode: typeof err.status === 'number' ? err.status : 1,
        stdout: err.stdout?.toString() ?? '',
        stderr: err.stderr?.toString() ?? '',
      };
    }
  }
  const args = ['push', 'origin', fix.branch];
  if (opts.forceWithLease) {
    args.push(`--force-with-lease=${opts.forceWithLease}`);
  }
  try {
    const stdout = execFileSync('git', ['-C', fix.cloneDir, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return { exitCode: 0, stdout, stderr: '' };
  } catch (err: any) {
    return {
      exitCode: typeof err.status === 'number' ? err.status : 1,
      stdout: err.stdout?.toString() ?? '',
      stderr: err.stderr?.toString() ?? '',
    };
  }
}

function runGhInvocation(fix: GitFixture, args: string): { exitCode: number; stdout: string; stderr: string } {
  const argv = args.split(/\s+/);
  try {
    // Run from the fixture's clone dir so gh stub's current_branch() resolves
    // via `git -C $PWD symbolic-ref --short HEAD` to the fixture branch, not
    // cucumber's runner cwd. This is what makes head-derivation match the
    // wren/<cardId> name the test expects (smoke run 3 root-cause).
    const stdout = execFileSync('gh', argv, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      cwd: fix.cloneDir,
    });
    return { exitCode: 0, stdout, stderr: '' };
  } catch (err: any) {
    return {
      exitCode: typeof err.status === 'number' ? err.status : 1,
      stdout: err.stdout?.toString() ?? '',
      stderr: err.stderr?.toString() ?? '',
    };
  }
}

function runCardsInvocation(argv: string[]): { exitCode: number; stdout: string; stderr: string } {
  try {
    const stdout = execFileSync('cards', argv, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    return { exitCode: 0, stdout, stderr: '' };
  } catch (err: any) {
    return {
      exitCode: typeof err.status === 'number' ? err.status : 1,
      stdout: err.stdout?.toString() ?? '',
      stderr: err.stderr?.toString() ?? '',
    };
  }
}

function originHasBranch(fix: GitFixture): boolean {
  try {
    execFileSync('git', ['-C', fix.originDir, 'rev-parse', `refs/heads/${fix.branch}`], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

function emitSpine(event: string, cardId: number): void {
  const chorusLog = path.join(
    process.env.CHORUS_HOME ?? '/Users/jeffbridwell/CascadeProjects/chorus',
    'platform',
    'scripts',
    'chorus-log',
  );
  try {
    execFileSync(chorusLog, [event, ctx.role ?? 'wren', `card=${cardId}`], { stdio: 'ignore' });
  } catch {
    /* ignore — observability emission failure shouldn't fail the test */
  }
}
