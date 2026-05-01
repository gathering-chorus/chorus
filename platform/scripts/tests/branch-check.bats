#!/usr/bin/env bats

# #2639 — branch-check.sh contract tests.
#
# Single-source for the role-prefix invariant. Sourced from git-queue.sh
# check_branch() (#2580 queue-path) and platform/hooks/pre-push (#2598
# bypass-path) so the regex lives in exactly one place.

setup() {
  CHECK="$BATS_TEST_DIRNAME/../branch-check.sh"
  [ -f "$CHECK" ] || skip "branch-check.sh not yet present"
}

@test "kade on kade/<card> branch passes" {
  run bash "$CHECK" kade kade/2639-pre-push
  [ "$status" -eq 0 ]
}

@test "kade on wren/<card> branch refused (cross-role)" {
  run bash "$CHECK" kade wren/2630-nudge-retirement-bats
  [ "$status" -ne 0 ]
  [[ "$output" == *"branch"* ]] || [[ "$output" == *"role"* ]]
}

@test "wren on main refused (must be on role-prefix branch)" {
  run bash "$CHECK" wren main
  [ "$status" -ne 0 ]
}

@test "silas on silas/<card> branch passes" {
  run bash "$CHECK" silas silas/2526-wave3-retry
  [ "$status" -eq 0 ]
}

@test "empty branch refused (detached HEAD case)" {
  run bash "$CHECK" kade ""
  [ "$status" -ne 0 ]
}

@test "unknown role refused" {
  run bash "$CHECK" frodo kade/123
  [ "$status" -ne 0 ]
}

@test "kade on kade/main-default passes (suffix-tolerant)" {
  run bash "$CHECK" kade kade/main-default
  [ "$status" -eq 0 ]
}

@test "git-queue.sh sources branch-check.sh (single-source verified)" {
  run grep -E "source.+branch-check\.sh|\\. .+branch-check\.sh" \
    "$BATS_TEST_DIRNAME/../git-queue.sh"
  [ "$status" -eq 0 ]
}

@test "pre-push hook sources branch-check.sh (single-source verified)" {
  run grep -E "source.+branch-check\.sh|\\. .+branch-check\.sh" \
    "$BATS_TEST_DIRNAME/../../hooks/pre-push"
  [ "$status" -eq 0 ]
}
