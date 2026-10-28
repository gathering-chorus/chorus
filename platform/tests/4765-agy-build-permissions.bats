#!/usr/bin/env bats
# @test-type: integration

setup() {
  load test_helper
  export WORKTREE="${CHORUS_ROOT:-.}"
}

@test "worktree services have group-writable permissions for chorus" {
  run test -w "$WORKTREE"
  [ "$status" -eq 0 ]
}

@test "Darwin ACLs allow group:chorus file and directory inheritance" {
  run ls -le -d "$WORKTREE"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "group:chorus" ]] || return 1
}

@test "headless user can create and remove build artifacts in target dir" {
  local target_dir="$WORKTREE/platform/services/chorus-hooks/target"
  mkdir -p "$target_dir"
  run touch "$target_dir/test-permission-check"
  [ "$status" -eq 0 ]
  rm -f "$target_dir/test-permission-check"
}
