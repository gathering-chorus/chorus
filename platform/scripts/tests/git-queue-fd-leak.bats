#!/usr/bin/env bats
# git-queue fd inheritance test — #1823
# Verifies git child processes don't inherit the lock fd

@test "git commit line closes fd 9 for child processes" {
  grep -q '9>&-' "$BATS_TEST_DIRNAME/../git-queue.sh"
}

@test "git add line closes fd 9 for child processes" {
  # Both git add and git commit must close fd 9
  local line
  line=$(grep 'git add.*git commit' "$BATS_TEST_DIRNAME/../git-queue.sh")
  echo "$line" | grep -q 'git add.*9>&-'
  echo "$line" | grep -q 'git commit.*9>&-'
}

@test "no credential-cache-daemon holding lock fd" {
  # After a commit, no daemon should have the lock file open
  local lock="/Users/jeffbridwell/CascadeProjects/chorus/.git-commit.lock"
  local holders
  holders=$(lsof "$lock" 2>/dev/null | grep -c 'credential-cache' || true)
  [ "$holders" -eq 0 ]
}
