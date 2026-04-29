#!/usr/bin/env bats
# git-queue-branch-check.bats — #2580
# What Jeff sees: under shared /chorus working tree, role A's git-queue commit
# lands on role B's branch because HEAD ≠ A's expected branch. The branch-check
# refuses cross-role commits at the queue layer; structural fix is per-role
# worktrees (#2582), this is defense-in-depth.

GIT_QUEUE="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/git-queue.sh"
[ -f "$GIT_QUEUE" ] || GIT_QUEUE="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/git-queue.sh"

setup() {
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet --initial-branch=main
  git config user.email "test@test"
  git config user.name "test"
  git commit --allow-empty -m "init" --quiet
  export REPO_ROOT="$TEST_REPO"
  export CHORUS_ROOT="$TEST_REPO"
}

teardown() {
  rm -rf "$TEST_REPO"
}

# --- AC2: rejects cross-role branch commits ---

@test "commit refused when HEAD=wren/X and DEPLOY_ROLE=kade" {
  git checkout -b wren/2575 --quiet
  echo "test" > file.txt
  export DEPLOY_ROLE="kade"

  run bash "$GIT_QUEUE" commit file.txt -- -m "kade: test"
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "branch" || (echo "expected branch-related error, got: $output" && false)
  echo "$output" | grep -qi "kade" || (echo "expected role 'kade' named in error, got: $output" && false)
  echo "$output" | grep -q "wren/2575" || (echo "expected actual branch named, got: $output" && false)
}

@test "commit refused when HEAD=main and DEPLOY_ROLE=kade (no role prefix)" {
  echo "test" > file.txt
  export DEPLOY_ROLE="kade"

  run bash "$GIT_QUEUE" commit file.txt -- -m "kade: test"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "kade/" || (echo "expected suggested prefix 'kade/' in error, got: $output" && false)
}

# --- AC6: passes when branch matches role ---

@test "commit allowed when HEAD=kade/X and DEPLOY_ROLE=kade" {
  git checkout -b kade/2580 --quiet
  echo "test" > file.txt
  export DEPLOY_ROLE="kade"

  # Don't run pre-commit hooks for this test (test repo has no hook infrastructure)
  run env _GIT_QUEUE_SKIP_HOOKS=1 bash "$GIT_QUEUE" commit file.txt -- -m "kade: test" --no-verify
  # The branch-check itself should not be the failure cause; if it fails
  # later for some other reason (no hooks installed), that's not our concern
  # as long as the branch-check passed
  if [ "$status" -ne 0 ]; then
    echo "$output" | grep -qi "branch" && (echo "branch-check should have passed for kade on kade/2580: $output" && false)
  fi
}

# --- AC1: DEPLOY_ROLE missing ---

@test "commit refused when DEPLOY_ROLE not set" {
  unset DEPLOY_ROLE
  git checkout -b kade/2580 --quiet
  echo "test" > file.txt

  run bash "$GIT_QUEUE" commit file.txt -- -m "test"
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "DEPLOY_ROLE\|role" || (echo "expected role-missing error, got: $output" && false)
}

# --- AC3: --force-branch escape hatch ---

@test "--force-branch flag bypasses the check" {
  git checkout -b some-other-branch --quiet
  echo "test" > file.txt
  export DEPLOY_ROLE="kade"

  run bash "$GIT_QUEUE" commit --force-branch file.txt -- -m "kade: emergency"
  # Branch-check should not be the failure cause
  if [ "$status" -ne 0 ]; then
    echo "$output" | grep -qi "expected branch" && (echo "force-branch should bypass: $output" && false)
  fi
}

# --- AC4: push has the same check ---

@test "push refused when HEAD=wren/X and DEPLOY_ROLE=kade" {
  git checkout -b wren/test --quiet
  export DEPLOY_ROLE="kade"

  run bash "$GIT_QUEUE" push
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "branch" || (echo "expected branch-related error on push, got: $output" && false)
}
