#!/usr/bin/env bats
# git-queue-push.bats — Tests for #1780 cross-role commit collision
# What Jeff sees: role commits but can't push because another role
# has unstaged changes. Jeff becomes the relay to unblock.

GIT_QUEUE="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/git-queue.sh"
[ -f "$GIT_QUEUE" ] || GIT_QUEUE="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/git-queue.sh"

# Use a temp git repo to avoid touching the real repo
setup() {
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet
  git commit --allow-empty -m "init" --quiet
  # Override REPO_ROOT so git-queue operates on test repo
  export REPO_ROOT="$TEST_REPO"
  export DEPLOY_ROLE="silas"
}

teardown() {
  rm -rf "$TEST_REPO"
}

# --- AC 1: push subcommand exists ---

@test "git-queue push subcommand exists" {
  run bash "$GIT_QUEUE" push
  # Should not say "unknown command"
  ! echo "$output" | grep -q "unknown command"
}

# --- AC 2: push succeeds with clean tree ---

@test "push does not error on clean tree without remote" {
  # No remote in test repo — push will fail on the push step
  # but should NOT fail on the stash/rebase step
  echo "test" > file.txt
  git add file.txt
  git commit -m "test commit" --quiet

  run bash "$GIT_QUEUE" push
  # Should not say "unstaged changes" — that's the bug we're fixing
  ! echo "$output" | grep -qi "unstaged changes"
}

# --- AC 3: push fails WITHOUT fix when dirty files exist ---
# This is the bug: git pull --rebase refuses when other roles have dirty files

@test "bare git pull --rebase fails with dirty files (proves the bug)" {
  echo "committed" > file.txt
  git add file.txt
  git commit -m "committed file" --quiet

  # Simulate another role's dirty file
  echo "dirty from kade" > kade-work.txt
  git add kade-work.txt
  echo "modified by kade" > kade-work.txt  # now it's modified-unstaged

  # Bare pull --rebase should fail — this IS the bug
  run git pull --rebase 2>&1
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "unstaged\|dirty\|cannot pull"
}

# --- AC 4: git-queue push handles dirty files without data loss ---

@test "git-queue push preserves other roles dirty files" {
  echo "committed" > file.txt
  git add file.txt
  git commit -m "committed file" --quiet

  # Simulate another role's dirty file
  echo "kade was here" > kade-dirty.txt

  run bash "$GIT_QUEUE" push

  # Kade's dirty file should still exist with same content
  [ -f kade-dirty.txt ]
  grep -q "kade was here" kade-dirty.txt
}

# --- AC 5: no manual stash required ---

@test "git-queue push does not require manual intervention" {
  echo "my commit" > silas-file.txt
  git add silas-file.txt
  git commit -m "silas work" --quiet

  # Dirty files from other roles
  echo "wren stuff" > wren-file.txt
  echo "kade stuff" > kade-file.txt

  # Push should handle it mechanically — no interactive prompts
  run timeout 10 bash "$GIT_QUEUE" push
  # Should not hang or require input
  [ "$status" -ne 124 ]  # 124 = timeout killed it
}

# --- #2597: silent-exit on clean tree ---
# Repro: clean tree, branch matches role, no upstream issues. Without the fix,
# `git status --porcelain | grep -v '^?' | head -1` returns 1 (no matches)
# under set -euo pipefail, which causes silent exit before any push attempt.
# With the fix, the substitution tolerates the no-match case and proceeds.

@test "push does not silent-exit on clean tree (#2597)" {
  # Set up a real upstream so push can complete
  ORIGIN=$(mktemp -d)
  git init --quiet --bare "$ORIGIN"
  git remote add origin "$ORIGIN"

  # Create a branch matching DEPLOY_ROLE for the #2580 branch-check
  git checkout -b silas/2597-test --quiet
  echo "init content" > seed.txt
  git add seed.txt
  git commit --quiet -m "silas: seed for 2597 test"
  git push -q -u origin silas/2597-test

  # Make one new commit so push has something to do, then verify clean tree
  echo "new" > new.txt
  git add new.txt
  git commit --quiet -m "silas: new commit for #2597 push"
  git status --porcelain  # should be empty (clean post-commit)

  # Run git-queue push — must NOT silent-exit
  run bash "$GIT_QUEUE" push
  # Diagnostic: print output if test fails
  if [ "$status" -ne 0 ]; then
    echo "exit=$status"
    echo "output=[$output]"
  fi
  # Either push succeeds (status 0) OR fails with VISIBLE output. Silent exit
  # 1 with empty output is the bug.
  if [ "$status" -ne 0 ]; then
    [ -n "$output" ] || (echo "FAIL: silent exit, empty output (#2597 bug present)" && false)
  fi
  rm -rf "$ORIGIN"
}

@test "dirty-check pipe tolerates clean tree (#2597 unit)" {
  # Direct unit test of the failing pipe in isolation. Under set -euo pipefail,
  # `printf "" | grep -v '^?' | head -1` should not propagate exit 1.
  run bash -c 'set -euo pipefail
    dirty=$(git status --porcelain 2>/dev/null | grep -v "^?" | head -1 || true)
    echo "dirty=[$dirty]"
    echo "after"'
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "after" || (echo "expected after-line, got: $output" && false)
}
