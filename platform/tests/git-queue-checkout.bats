#!/usr/bin/env bats
# git-queue-checkout.bats — Tests for #2710 typed checkout/switch/branch adapter.
# Candidate A from #2706 Mode-A close: roles route working-tree mutation through
# git-queue.sh's flock so concurrent peers serialize at the lock.

GIT_QUEUE="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/git-queue.sh"
[ -f "$GIT_QUEUE" ] || GIT_QUEUE="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/git-queue.sh"

setup() {
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet
  git config user.email "t@t" && git config user.name "t"
  git commit --allow-empty -m "init" --quiet
  # Base commit to give checkout-by-SHA something real to grab.
  echo "v1" > file.txt
  git add file.txt
  git commit --quiet -m "v1"
  BASE_SHA=$(git rev-parse HEAD)
  echo "v2" > file.txt
  git commit --quiet -am "v2"
  git checkout -q -b kade/test-existing
  git checkout -q $(git rev-parse --abbrev-ref HEAD | head -1)
  git checkout -q master 2>/dev/null || git checkout -q main
  export REPO_ROOT="$TEST_REPO"
  export DEPLOY_ROLE="kade"
  # git-queue.sh resolves CHORUS_LOG from REPO_ROOT/platform/scripts/chorus-log.
  mkdir -p "$TEST_REPO/platform/scripts"
  cat > "$TEST_REPO/platform/scripts/chorus-log" <<EOF
#!/bin/bash
echo "\$@" >> "${TEST_REPO}/.spine-events"
EOF
  chmod +x "$TEST_REPO/platform/scripts/chorus-log"
}

teardown() {
  rm -rf "$TEST_REPO"
}

# AC1: switch to existing branch
@test "git-queue checkout switches to existing local branch" {
  run bash "$GIT_QUEUE" checkout --force-branch kade/test-existing
  [ "$status" -eq 0 ]
  CURRENT=$(git symbolic-ref --short HEAD)
  [ "$CURRENT" = "kade/test-existing" ]
}

# AC1+AC3: emits build.checkout.completed on success
@test "git-queue checkout emits build.checkout.completed on success" {
  bash "$GIT_QUEUE" checkout --force-branch kade/test-existing 2>&1
  run cat "$TEST_REPO/.spine-events"
  [[ "$output" == *"build.checkout.completed"* ]]
}

# AC1: -b creates new branch
@test "git-queue checkout -b creates new branch and switches" {
  run bash "$GIT_QUEUE" checkout --force-branch -b kade/test-new
  [ "$status" -eq 0 ]
  CURRENT=$(git symbolic-ref --short HEAD)
  [ "$CURRENT" = "kade/test-new" ]
}

# AC1: SHA -- file restores file content (Mode-A recovery shape)
@test "git-queue checkout SHA -- file restores file from ref" {
  echo "scratch" > file.txt
  run bash "$GIT_QUEUE" checkout --force-branch "$BASE_SHA" -- file.txt
  [ "$status" -eq 0 ]
  run cat file.txt
  [ "$output" = "v1" ]
}

# AC4: missing branch fails clean with wrapper diagnostic
@test "git-queue checkout missing branch exits non-zero with wrapper stderr" {
  run bash "$GIT_QUEUE" checkout --force-branch kade/never-existed
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "checkout failed"
}

# AC3: missing branch emits build.checkout.failed
@test "git-queue checkout failure emits build.checkout.failed" {
  bash "$GIT_QUEUE" checkout --force-branch kade/never-existed 2>&1 || true
  run cat "$TEST_REPO/.spine-events"
  [[ "$output" == *"build.checkout.failed"* ]]
}

# AC5: already-on-branch is no-op (succeeds, no spine event)
@test "git-queue checkout on already-current branch is no-op" {
  CURRENT=$(git symbolic-ref --short HEAD)
  rm -f "$TEST_REPO/.spine-events"
  run bash "$GIT_QUEUE" checkout --force-branch "$CURRENT"
  [ "$status" -eq 0 ]
  # No spine event for no-op (mirrors do_pull's behavior on Already up-to-date)
  if [ -f "$TEST_REPO/.spine-events" ]; then
    run cat "$TEST_REPO/.spine-events"
    [[ "$output" != *"build.checkout.completed"* ]]
  fi
}

# AC2: switch subcommand mirrors checkout for existing branch
@test "git-queue switch subcommand routes through same lock" {
  run bash "$GIT_QUEUE" switch --force-branch kade/test-existing
  [ "$status" -eq 0 ]
  CURRENT=$(git symbolic-ref --short HEAD)
  [ "$CURRENT" = "kade/test-existing" ]
}

# AC2: branch subcommand creates new branch without switching
@test "git-queue branch subcommand creates new branch" {
  run bash "$GIT_QUEUE" branch --force-branch kade/test-branch-only
  [ "$status" -eq 0 ]
  run git branch --list kade/test-branch-only
  [ -n "$output" ]
}
