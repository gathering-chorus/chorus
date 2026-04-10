#!/usr/bin/env bats
# git-queue-push.bats — Tests for #1780 cross-role commit collision
# What Jeff sees: role commits but can't push because another role
# has unstaged changes. Jeff becomes the relay to unblock.

GIT_QUEUE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/git-queue.sh"

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
