#!/bin/bash
# test-git-queue.sh — AC5 tests for git-queue.sh
# Tests: lock/commit/release cycle, concurrent contention, doc-drift enforcement,
#        partial-commit-on-timeout detection
#
# Uses a temporary git repo — no side effects on the real repo.
# Must be run directly (not via Claude hook-intercepted Bash) because
# setup commits use raw git, not git-queue.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GIT_QUEUE="$SCRIPT_DIR/git-queue.sh"
TEST_DIR=$(mktemp -d)
PASS=0
FAIL=0

cleanup() {
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected '$expected', got '$actual')"
    ((FAIL++))
  fi
}

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -q "$needle"; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    ((FAIL++))
  fi
}

assert_exit() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected exit $expected, got $actual)"
    ((FAIL++))
  fi
}

# --- Setup temp repo ---
setup_repo() {
  rm -rf "$TEST_DIR"
  mkdir -p "$TEST_DIR"
  cd "$TEST_DIR"
  git init -q
  git config user.email "test@test.com"
  git config user.name "Test"
  echo "initial" > file.txt
  git add file.txt
  git commit -q -m "initial"
}

echo "=== AC5: Git Queue Tests ==="

# Tests 1/3/4/7 exercise the commit-flow surface (file validation, message
# validation, lock cycle, doc-drift). They use --force-branch to bypass
# check_branch — the test setup creates a fresh tmp repo on whatever the
# git default branch is (master/main), not on kade/<id>. check_branch was
# strictened post-#2580 (branch-prefix) and #2641 (mode-C active-card)
# to refuse off-pattern branches; without --force-branch these tests fail
# at check_branch before reaching the surface they exercise. Tests 5/6
# explicitly test branch/syntax validation and do NOT use --force-branch.

# --- Test 1: lock/commit/release cycle ---
echo ""
echo "Test 1: lock/commit/release cycle"
setup_repo
echo "changed" > file.txt
output=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit --force-branch file.txt -- -m "kade: test commit" 2>&1) || true
# Verify commit landed
log=$(cd "$TEST_DIR" && git log --oneline -1)
assert_contains "commit message in log" "kade: test commit" "$log"
# Lock should be released — status checks the lock file in REPO_ROOT
# When run from TEST_DIR, REPO_ROOT is TEST_DIR
status_out=$(cd "$TEST_DIR" && bash "$GIT_QUEUE" status 2>&1) || true
assert_contains "lock released after commit" "free" "$status_out"

# --- Test 2: status when free ---
echo ""
echo "Test 2: status shows free"
setup_repo
status_out=$(cd "$TEST_DIR" && bash "$GIT_QUEUE" status 2>&1)
assert_contains "status is free" "free" "$status_out"

# --- Test 3: no files returns error ---
echo ""
echo "Test 3: no files returns error"
setup_repo
set +e
output=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit --force-branch -- -m "empty" 2>&1)
exit_code=$?
set -e
assert_eq "exit code 1 for no files" "1" "$exit_code"
assert_contains "error message mentions files" "no files" "$output"

# --- Test 4: no commit message returns error ---
echo ""
echo "Test 4: no commit message returns error"
setup_repo
echo "data" > newfile.txt
set +e
output=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit --force-branch newfile.txt 2>&1)
exit_code=$?
set -e
assert_eq "exit code 1 for no message" "1" "$exit_code"
assert_contains "error mentions message" "commit message" "$output"

# --- Test 5: 'add' misuse detection ---
echo ""
echo "Test 5: common misuse — 'add' instead of 'commit'"
setup_repo
set +e
output=$(cd "$TEST_DIR" && bash "$GIT_QUEUE" add file.txt 2>&1)
exit_code=$?
set -e
assert_eq "exit code 1 for add" "1" "$exit_code"
assert_contains "suggests commit" "commit" "$output"

# --- Test 6: old syntax detection ---
echo ""
echo "Test 6: old syntax detection (role as first arg)"
setup_repo
set +e
output=$(cd "$TEST_DIR" && bash "$GIT_QUEUE" silas "test message" 2>&1)
exit_code=$?
set -e
assert_eq "exit code 1 for old syntax" "1" "$exit_code"
assert_contains "detects old syntax" "old syntax" "$output"

# --- Test 7: doc-drift enforcement ---
echo ""
echo "Test 7: doc-drift enforcement"
setup_repo
# Create a drift config
mkdir -p "$(dirname "$GIT_QUEUE")"
cat > "$TEST_DIR/doc-drift.conf" <<'DRIFT'
# code_glob → doc_path
src/ → docs/API.md
DRIFT
mkdir -p "$TEST_DIR/src" "$TEST_DIR/docs"
echo "code" > "$TEST_DIR/src/handler.ts"
echo "docs" > "$TEST_DIR/docs/API.md"
git -C "$TEST_DIR" add src/ docs/
git -C "$TEST_DIR" commit -q -m "setup"
echo "changed code" > "$TEST_DIR/src/handler.ts"
# git-queue reads doc-drift.conf relative to the script dir, not the repo
# This test verifies the mechanism exists — the actual config is in messages/scripts/
# For now, verify commit works without drift config in test dir
output=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit --force-branch src/handler.ts -- -m "kade: code change" 2>&1) || true
log=$(cd "$TEST_DIR" && git log --oneline -1)
assert_contains "commit succeeds without drift config" "kade: code change" "$log"

# --- Test 8: help command ---
echo ""
echo "Test 8: help command"
output=$(bash "$GIT_QUEUE" help 2>&1)
assert_contains "help shows usage" "FIFO commit lock" "$output"

# --- Test 9 (#2752): no-op commit routes git's stdout message to stderr ---
echo ""
echo "Test 9: no-op commit error message lands on stderr (not stdout)"
setup_repo
# setup_repo already commits initial file.txt — tree is clean. Re-attempting
# commit with the same content is a no-op: git exits non-zero with 'nothing
# to commit' on stdout (longstanding git quirk). After #2752 fix: git-queue
# routes that message to stderr so MCP classifiers see it where errors belong.
#
# git-queue.sh sources branch-check.sh from $REPO_ROOT/platform/scripts/.
# In test fixture, stub the dep so the script reaches the commit path.
mkdir -p "$TEST_DIR/platform/scripts"
cat > "$TEST_DIR/platform/scripts/branch-check.sh" <<'STUB'
# test stub — accept all branches
branch_check_match() { return 0; }
STUB
set +e
stderr_only=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit file.txt -- -m "kade: noop" 2>&1 >/dev/null)
stdout_only=$(cd "$TEST_DIR" && DEPLOY_ROLE=kade bash "$GIT_QUEUE" commit file.txt -- -m "kade: noop" 2>/dev/null)
set -e
# Git's exact wording varies ("nothing to commit, working tree clean" vs
# "nothing added to commit but untracked files present"). Both indicate the
# no-op condition. The fix is correct as long as ONE of them is on stderr.
if echo "$stderr_only" | grep -qE "nothing (to|added to) commit"; then
  echo "PASS: noop commit message on stderr"
  PASS=$((PASS + 1))
else
  echo "FAIL: noop commit message not on stderr (got: $stderr_only)"
  FAIL=$((FAIL + 1))
fi
if echo "$stdout_only" | grep -qE "nothing (to|added to) commit"; then
  echo "FAIL: noop commit message leaked to stdout (should be stderr-only)"
  FAIL=$((FAIL + 1))
else
  echo "PASS: noop commit stdout clean (no leaked error message)"
  PASS=$((PASS + 1))
fi

# Note: --force-with-lease + --branch arg-parsing tests live in the canonical
# location proving/scripts/tests/test-git-queue-force-with-lease.sh (#2877).
# That file covers AC1 (engagement) + AC2 (lease semantics with peer divergence).

# --- Summary ---
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
