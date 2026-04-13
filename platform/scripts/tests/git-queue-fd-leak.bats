#!/usr/bin/env bats
# git-queue fd inheritance test — #1823
# Verifies git child processes don't inherit the lock fd

@test "git commit line closes fd 9 for child processes" {
  grep -q '9>&-' "$BATS_TEST_DIRNAME/../git-queue.sh"
}

@test "git add and commit close fd 9 for child processes" {
  local line
  line=$(grep 'git add.*git commit' "$BATS_TEST_DIRNAME/../git-queue.sh")
  echo "$line" | grep -q 'git add.*9>&-'
  echo "$line" | grep -q 'git commit.*9>&-'
}

@test "push path closes fd 9 on stash, pull, push, and stash pop" {
  local script="$BATS_TEST_DIRNAME/../git-queue.sh"
  grep -q 'stash --quiet 9>&-' "$script"
  grep -q 'pull --rebase 9>&-' "$script"
  grep -q 'push 9>&-' "$script"
  grep -q 'stash pop --quiet 9>&-' "$script"
}

@test "ontology validation closes fd 9" {
  grep -q 'validate_script.*9>&-' "$BATS_TEST_DIRNAME/../git-queue.sh"
}

@test "no credential-cache-daemon holding lock fd" {
  # After a commit, no daemon should have the lock file open
  local lock="/Users/jeffbridwell/CascadeProjects/chorus/.git-commit.lock"
  local holders
  holders=$(lsof "$lock" 2>/dev/null | grep -c 'credential-cache' || true)
  [ "$holders" -eq 0 ]
}
