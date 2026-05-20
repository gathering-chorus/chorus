// #3011 — chorus_commit must not hide the real git error behind pre-commit's
// success output. When pre-commit passes (🟢 N/N) but `git commit` fails
// downstream, git flushes the hook's stdout into its own stderr, so stderr's
// FIRST line is the green success line. The old refusal used
// `stderr.split('\n')[0]` and surfaced "commit-fail — pre-commit: 🟢 2/2
// checks passed" — a contradiction that hid the actual cause (observed live
// 2026-05-20). commitFailureDetail() picks the real failure line instead.
//
// classifyCommitFailure() is unchanged (#2699) and re-tested here for the
// AC3 regression: a genuine hook failure must still classify as hook-fail and
// keep its failure detail.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { commitFailureDetail, classifyCommitFailure, findMissingPaths } from '../src/server';

test('AC1: non-existent paths are returned as missing', () => {
  const present = new Set(['src/server.ts', 'README.md']);
  const missing = findMissingPaths(
    ['src/server.ts', 'src/typo-not-here.ts', 'README.md'],
    (p) => present.has(p),
  );
  assert.deepEqual(missing, ['src/typo-not-here.ts']);
});

test('AC1: all-present paths yield no missing (untracked new files exist on disk → pass)', () => {
  const onDisk = new Set(['a.ts', 'brand-new-untracked.ts']);
  assert.deepEqual(findMissingPaths(['a.ts', 'brand-new-untracked.ts'], (p) => onDisk.has(p)), []);
});

test('AC2: pre-commit success line is stripped from the refusal detail', () => {
  const stderr = [
    'pre-commit: 🟢 2/2 checks passed',
    "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
  ].join('\n');
  const detail = commitFailureDetail(stderr);
  assert.ok(!/🟢/.test(detail), `detail must not contain 🟢: ${detail}`);
  assert.ok(!/passed/i.test(detail), `detail must not contain "passed": ${detail}`);
  assert.match(detail, /could not read Username/);
});

test('AC2: multiple pre-commit success/progress lines are all skipped', () => {
  const stderr = [
    'pre-commit: running 3 checks',
    'pre-commit: 🟢 3/3 checks passed',
    'error: pathspec did not match — nothing to commit downstream',
  ].join('\n');
  const detail = commitFailureDetail(stderr);
  assert.match(detail, /pathspec did not match/);
  assert.ok(!/🟢|passed/i.test(detail), `detail leaked success noise: ${detail}`);
});

test('AC3: a genuine pre-commit FAILURE line is preserved as the detail', () => {
  const stderr = 'pre-commit: 🔴 1/2 checks failed — clippy-ratchet blocked';
  // Still classifies as hook-fail (#2699 marker on the line).
  assert.equal(classifyCommitFailure(stderr), 'hook-fail');
  // And the failure detail is kept, not stripped.
  const detail = commitFailureDetail(stderr);
  assert.match(detail, /clippy-ratchet blocked/);
});

test('AC2/AC3: pre-commit pass still classifies downstream failure as commit-fail', () => {
  const stderr = 'pre-commit: 🟢 2/2 checks passed\nfatal: unable to write new index file';
  assert.equal(classifyCommitFailure(stderr), 'commit-fail');
});

test('falls back to the first non-empty line when there is no pre-commit noise', () => {
  const stderr = '\nfatal: unable to write new index file\n';
  assert.equal(commitFailureDetail(stderr), 'fatal: unable to write new index file');
});
