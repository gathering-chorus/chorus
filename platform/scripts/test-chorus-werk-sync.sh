#!/usr/bin/env bash
# test-chorus-werk-sync.sh — tests for chorus-werk-sync (#2735).
#
# Sync hook does lock-guarded `git pull --ff-only origin main` on canonical.
# Refuses when canonical isn't on main, mid-merge/rebase/cherry-pick, or
# when a worktree has an in-progress operation. Tests run against a temp
# repo with a fake remote; no side effects on real /chorus.
#
# Run directly (not via Claude hook-intercepted Bash) since setup uses
# raw git, not git-queue.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_WERK_SYNC="$SCRIPT_DIR/chorus-werk-sync"

PASS=0
FAIL=0

if [ ! -x "$CHORUS_WERK_SYNC" ]; then
  echo "FAIL: chorus-werk-sync not found or not executable at $CHORUS_WERK_SYNC"
  exit 1
fi

TEST_ROOT=$(mktemp -d)
REMOTE="$TEST_ROOT/remote.git"
CANONICAL="$TEST_ROOT/chorus"
WERK_BASE="$TEST_ROOT/chorus-werk"

cleanup() {
  if [ -d "$CANONICAL/.git" ]; then
    for wt in "$WERK_BASE"/*/; do
      [ -d "$wt" ] || continue
      git -C "$CANONICAL" worktree remove --force "$wt" 2>/dev/null || true
    done
  fi
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

# Set up: bare remote + canonical clone + 1 commit on main
git init -q --bare "$REMOTE"
git clone -q "$REMOTE" "$CANONICAL"
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
git -C "$CANONICAL" checkout -q -b main 2>/dev/null || git -C "$CANONICAL" checkout -q main
echo "1" > "$CANONICAL/file.txt"
git -C "$CANONICAL" add file.txt
git -C "$CANONICAL" commit -q -m "1"
git -C "$CANONICAL" push -q origin main 2>/dev/null

# Make a second commit on the remote (simulating a peer's PR landing)
PEER=$(mktemp -d)
git clone -q "$REMOTE" "$PEER"
git -C "$PEER" config user.email "peer@chorus.local"
git -C "$PEER" config user.name "peer"
echo "2" > "$PEER/file.txt"
git -C "$PEER" add file.txt
git -C "$PEER" commit -q -m "2"
git -C "$PEER" push -q origin main
rm -rf "$PEER"

# Override env
export CHORUS_HOME="$CANONICAL"
export CHORUS_WERK_BASE="$WERK_BASE"

assert() {
  local label="$1"; shift
  if "$@"; then
    PASS=$((PASS + 1))
    echo "PASS: $label"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL: $label"
  fi
}

# --- TEST 1: clean canonical on main → sync succeeds, ff to peer's commit ---
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-stdout.$$ 2>&1
RC=$?
assert "clean sync exits 0" test "$RC" -eq 0
TIP=$(git -C "$CANONICAL" rev-parse HEAD)
PEER_TIP=$(git -C "$REMOTE" rev-parse main)
assert "canonical fast-forwarded to remote main" test "$TIP" = "$PEER_TIP"

# --- TEST 2: not on main → sync refuses ---
git -C "$CANONICAL" checkout -q -b stray
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-stderr.$$ 2>&1
RC=$?
assert "sync refuses when canonical not on main" test "$RC" -ne 0
git -C "$CANONICAL" checkout -q main 2>/dev/null
git -C "$CANONICAL" branch -q -D stray 2>/dev/null

# --- TEST 3: mid-rebase → sync refuses ---
# Simulate by creating .git/REBASE_HEAD
mkdir -p "$CANONICAL/.git/rebase-merge"
touch "$CANONICAL/.git/rebase-merge/onto"
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-out3.$$ 2>&1
RC=$?
assert "sync refuses during in-flight rebase" test "$RC" -ne 0
rm -rf "$CANONICAL/.git/rebase-merge"

# --- TEST 4: mid-merge → sync refuses ---
echo "fake-merge" > "$CANONICAL/.git/MERGE_HEAD"
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-out4.$$ 2>&1
RC=$?
assert "sync refuses during in-flight merge" test "$RC" -ne 0
rm -f "$CANONICAL/.git/MERGE_HEAD"

# --- TEST 5: a worktree has an index.lock → sync refuses ---
mkdir -p "$WERK_BASE/kade"
git -C "$CANONICAL" worktree add --detach "$WERK_BASE/kade" main >/dev/null 2>&1
touch "$CANONICAL/.git/worktrees/kade/index.lock"
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-out5.$$ 2>&1
RC=$?
assert "sync refuses when a worktree is mid-commit (index.lock)" test "$RC" -ne 0
rm -f "$CANONICAL/.git/worktrees/kade/index.lock"

# --- TEST 6: clean state again → sync is a no-op (ff-only, already up-to-date) ---
"$CHORUS_WERK_SYNC" > /tmp/cw-sync-out6.$$ 2>&1
RC=$?
assert "sync exits 0 when already up-to-date" test "$RC" -eq 0

rm -f /tmp/cw-sync-*.$$ 2>/dev/null

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
