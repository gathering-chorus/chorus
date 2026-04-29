#!/usr/bin/env bats
# werk-substrate.bats — #2598 substrate uniformity
# What Jeff sees: all three roles execute the same way for build/deploy/check.
# These tests cover the wrapper + pre-push hook.

WERK="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/werk"
[ -x "$WERK" ] || WERK="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/werk"

PRE_PUSH="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/hooks/pre-push"
[ -x "$PRE_PUSH" ] || PRE_PUSH="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../hooks" && pwd)/pre-push"

# --- werk check ---

@test "werk check exits 0 and emits drift report" {
  run bash "$WERK" check
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "drift" || (echo "expected 'drift' in output: $output" && false)
  echo "$output" | grep -q "git HEAD" || (echo "expected git state in output: $output" && false)
}

@test "werk check is read-only (no files modified)" {
  # Snapshot mtime of canonical binary if it exists
  local shim="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  if [ -f "$shim" ]; then
    local before_mtime
    before_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
    run bash "$WERK" check
    local after_mtime
    after_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
    [ "$before_mtime" = "$after_mtime" ] || (echo "werk check mutated the binary mtime" && false)
  fi
}

@test "werk help shows substrate framing" {
  run bash "$WERK" help
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "execute work-units against the chorus substrate"
}

# --- werk deploy refusal (no main checkout) ---

@test "werk deploy refuses when HEAD != origin/main" {
  # We're on kade/2598-* branch by definition while this card is in flight,
  # so HEAD will not match origin/main. Verify werk deploy refuses.
  run bash "$WERK" deploy
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "main\|HEAD" || (echo "expected main/HEAD diagnostic, got: $output" && false)
}

# --- pre-push hook ---

setup_pre_push_test() {
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet --initial-branch=main
  git config user.email t@t
  git config user.name t
  bash -c 'GIT_AUTHOR_DATE="2026-01-01" git commit --allow-empty -q -m init' 2>&1 || true
}

@test "pre-push refuses without _GIT_QUEUE_PUSH marker" {
  # Direct invocation of the hook (simulating raw git push)
  unset _GIT_QUEUE_PUSH
  unset DEPLOY_ROLE_PREPUSH_OVERRIDE
  export DEPLOY_ROLE=kade

  # Simulate stdin that git push would provide: <local-ref> <local-sha> <remote-ref> <remote-sha>
  run bash -c "echo 'refs/heads/kade/test 1234567890abcdef refs/heads/kade/test 0000000000000000000000000000000000000000' | bash $PRE_PUSH"
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "raw 'git push' refused\|git-queue" || (echo "expected raw-push refusal, got: $output" && false)
}

@test "pre-push refuses wrong-role branch even with marker" {
  export DEPLOY_ROLE=kade
  export _GIT_QUEUE_PUSH=1
  unset DEPLOY_ROLE_PREPUSH_OVERRIDE

  # Pre-push reads cwd's HEAD via git symbolic-ref. Make a tmp repo on a wren branch.
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet --initial-branch=main
  git config user.email t@t; git config user.name t
  git commit --allow-empty -q -m init
  git checkout -q -b wren/test

  run bash -c "echo 'refs/heads/wren/test 1234567890abcdef refs/heads/wren/test 0000000000000000000000000000000000000000' | bash $PRE_PUSH"
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "branch.*does not match\|prefix" || (echo "expected branch-prefix error, got: $output" && false)

  rm -rf "$TEST_REPO"
}

@test "pre-push allows correct role + marker" {
  export DEPLOY_ROLE=kade
  export _GIT_QUEUE_PUSH=1
  unset DEPLOY_ROLE_PREPUSH_OVERRIDE

  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet --initial-branch=main
  git config user.email t@t; git config user.name t
  git commit --allow-empty -q -m init
  git checkout -q -b kade/test

  run bash -c "echo 'refs/heads/kade/test 1234567890abcdef refs/heads/kade/test 0000000000000000000000000000000000000000' | bash $PRE_PUSH"
  [ "$status" -eq 0 ]

  rm -rf "$TEST_REPO"
}

@test "pre-push override bypass logs warning but allows" {
  export DEPLOY_ROLE=kade
  unset _GIT_QUEUE_PUSH
  export DEPLOY_ROLE_PREPUSH_OVERRIDE=1

  run bash -c "echo 'refs/heads/anything 1234567890abcdef refs/heads/anything 0000000000000000000000000000000000000000' | bash $PRE_PUSH"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi "WARN\|override\|bypass" || (echo "expected override warning, got: $output" && false)
}

@test "pre-push allows delete (zero local-sha)" {
  export DEPLOY_ROLE=kade
  unset _GIT_QUEUE_PUSH
  unset DEPLOY_ROLE_PREPUSH_OVERRIDE

  # Zero local-sha = delete; should be allowed regardless
  run bash -c "echo '(delete) 0000000000000000000000000000000000000000 refs/heads/anything 1234567890abcdef' | bash $PRE_PUSH"
  [ "$status" -eq 0 ]
}
