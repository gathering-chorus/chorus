#!/usr/bin/env bats
# git-queue-delete-remote.bats — Tests for #2701 typed remote-branch deletion.
# What roles see: `git-queue.sh push --delete <branch>` deletes via the queue,
# sets the pre-push marker, logs spine event, refuses cleanly when missing.

GIT_QUEUE="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/git-queue.sh"
[ -f "$GIT_QUEUE" ] || GIT_QUEUE="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/git-queue.sh"

setup() {
  TEST_REMOTE=$(mktemp -d)
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REMOTE"
  git init --quiet --bare
  cd "$TEST_REPO"
  git init --quiet
  git config user.email "t@t" && git config user.name "t"
  git commit --allow-empty -m "init" --quiet
  git remote add origin "$TEST_REMOTE"
  git push --quiet -u origin master 2>/dev/null || git push --quiet -u origin main
  export REPO_ROOT="$TEST_REPO"
  export DEPLOY_ROLE="kade"
  # git-queue.sh resolves CHORUS_LOG from REPO_ROOT/platform/scripts/chorus-log,
  # not env. Stub at that path so log_event has somewhere to call into.
  mkdir -p "$TEST_REPO/platform/scripts"
  cat > "$TEST_REPO/platform/scripts/chorus-log" <<EOF
#!/bin/bash
echo "\$@" >> "${TEST_REPO}/.spine-events"
EOF
  chmod +x "$TEST_REPO/platform/scripts/chorus-log"
}

teardown() {
  rm -rf "$TEST_REPO" "$TEST_REMOTE"
}

# AC1: --delete <branch> deletes the remote ref via the queue
@test "git-queue push --delete deletes existing remote branch" {
  git checkout -q -b kade/test-delete
  git commit --allow-empty -m "test" --quiet
  git push --quiet -u origin kade/test-delete

  # Verify branch exists on remote
  run git ls-remote --heads origin kade/test-delete
  [ -n "$output" ]

  # Delete via git-queue
  run bash "$GIT_QUEUE" push --delete kade/test-delete
  [ "$status" -eq 0 ]

  # Verify branch gone from remote
  run git ls-remote --heads origin kade/test-delete
  [ -z "$output" ]
}

# AC2: _GIT_QUEUE_PUSH=1 is set so pre-push hook accepts the push
@test "git-queue push --delete sets _GIT_QUEUE_PUSH marker" {
  git checkout -q -b kade/marker-test
  git commit --allow-empty -m "marker" --quiet
  git push --quiet -u origin kade/marker-test

  # Install a fake pre-push hook in the local repo that records the marker value
  HOOK_DIR="$TEST_REPO/.git/hooks"
  cat > "$HOOK_DIR/pre-push" <<'EOF'
#!/bin/bash
echo "_GIT_QUEUE_PUSH=${_GIT_QUEUE_PUSH:-unset}" >> "${REPO_ROOT}/.pre-push-marker"
exit 0
EOF
  chmod +x "$HOOK_DIR/pre-push"

  bash "$GIT_QUEUE" push --delete kade/marker-test 2>&1

  run cat "$TEST_REPO/.pre-push-marker"
  [[ "$output" == *"_GIT_QUEUE_PUSH=1"* ]]
}

# AC3: Spine event branch.deleted emitted on success
@test "git-queue push --delete emits branch.deleted spine event" {
  git checkout -q -b kade/spine-test
  git commit --allow-empty -m "spine" --quiet
  git push --quiet -u origin kade/spine-test

  bash "$GIT_QUEUE" push --delete kade/spine-test 2>&1

  run cat "$TEST_REPO/.spine-events"
  [[ "$output" == *"branch.deleted"* ]]
  [[ "$output" == *"kade/spine-test"* ]]
}

# AC4: Refuses gracefully if branch doesn't exist on remote (no destructive force)
@test "git-queue push --delete refuses missing branch with clean error" {
  run bash "$GIT_QUEUE" push --delete kade/never-existed
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "does not exist"
}

# AC5: Branch with commits not in main: warns (proceeds, doesn't refuse —
# cleanup may legitimately want to drop branches that never landed).
@test "git-queue push --delete warns on branch with unique commits" {
  git checkout -q -b kade/has-unique
  echo "unique" > unique.txt
  git add unique.txt
  git commit -q -m "unique commit"
  git push --quiet -u origin kade/has-unique

  run bash "$GIT_QUEUE" push --delete kade/has-unique
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi "unique\|unmerged\|warn"
}

# Regression: --delete without branch arg returns clean error
@test "git-queue push --delete without branch arg errors" {
  run bash "$GIT_QUEUE" push --delete
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "branch\|usage\|requires"
}
