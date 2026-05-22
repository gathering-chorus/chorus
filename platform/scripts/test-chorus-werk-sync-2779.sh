#!/usr/bin/env bash
# test-chorus-werk-sync-2779.sh — tests for #2779 detached-HEAD recovery.
#
# Adds three test cases on top of the #2735 baseline:
#   - detached canonical → sync aborts with recovery hint
#   - `chorus-werk-sync repair` on detached canonical → recovers atomically
#   - `repair` is idempotent on already-attached canonical
#
# Same pattern as test-chorus-werk-sync.sh: temp repo + fake remote.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_WERK_SYNC="$SCRIPT_DIR/chorus-werk-sync"

PASS=0
FAIL=0

if [ ! -x "$CHORUS_WERK_SYNC" ]; then
  echo "FAIL: chorus-werk-sync not found at $CHORUS_WERK_SYNC"
  exit 1
fi

TEST_ROOT=$(mktemp -d)
REMOTE="$TEST_ROOT/remote.git"
CANONICAL="$TEST_ROOT/chorus"
trap 'rm -rf "$TEST_ROOT"' EXIT

git init -q --bare "$REMOTE"
# #3033: init canonical directly on main + add remote, instead of cloning the
# empty bare repo (whose HEAD follows init.defaultBranch and left the first
# commit off main, failing `push origin main`).
git init -q "$CANONICAL"
git -C "$CANONICAL" symbolic-ref HEAD refs/heads/main
git -C "$CANONICAL" remote add origin "$REMOTE"
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
echo "1" > "$CANONICAL/file.txt"
git -C "$CANONICAL" add file.txt
git -C "$CANONICAL" commit -q -m "1"
git -C "$CANONICAL" push -q -u origin main
# bare remote HEAD → main so the peer clone below lands on main
git -C "$REMOTE" symbolic-ref HEAD refs/heads/main

# Peer ahead — gives repair something to ff to.
PEER=$(mktemp -d)
git clone -q "$REMOTE" "$PEER"
git -C "$PEER" config user.email "peer@chorus.local"
git -C "$PEER" config user.name "peer"
echo "2" > "$PEER/file.txt"
git -C "$PEER" add file.txt
git -C "$PEER" commit -q -m "2"
git -C "$PEER" push -q origin main
rm -rf "$PEER"

export CHORUS_HOME="$CANONICAL"
export CHORUS_WERK_BASE="$TEST_ROOT/chorus-werk"

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

# --- TEST 1: bare invocation prints usage pointing to repair for recovery ----
# #2863: sync is automatic inside /build; the standalone script exposes only
# repair/recover, so a bare call prints usage (exit 2) rather than running a
# default sync. The usage must still guide a detached canonical to `repair`.
git -C "$CANONICAL" checkout -q --detach HEAD
"$CHORUS_WERK_SYNC" > "$TEST_ROOT/out1.log" 2>&1
RC=$?
assert "bare invocation exits non-zero (usage)" test "$RC" -ne 0
assert "usage points to repair for detached-HEAD recovery" grep -qi 'detached-HEAD recovery' "$TEST_ROOT/out1.log"
git -C "$CANONICAL" checkout -q main 2>/dev/null

# --- TEST 2: chorus-werk-sync repair on detached canonical → recovers --------
git -C "$CANONICAL" checkout -q --detach HEAD
"$CHORUS_WERK_SYNC" repair > "$TEST_ROOT/out2.log" 2>&1
RC=$?
assert "repair exits 0 on detached canonical" test "$RC" -eq 0
HEAD_AFTER=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")
assert "repair re-attaches HEAD to main" test "$HEAD_AFTER" = "main"
LOCAL_MAIN=$(git -C "$CANONICAL" rev-parse main)
ORIGIN_MAIN=$(git -C "$CANONICAL" rev-parse origin/main)
assert "repair fast-forwards local main to origin/main" test "$LOCAL_MAIN" = "$ORIGIN_MAIN"

# --- TEST 3: repair on already-attached canonical → idempotent (no-op) -------
"$CHORUS_WERK_SYNC" repair > "$TEST_ROOT/out3.log" 2>&1
RC=$?
assert "repair is idempotent on attached canonical" test "$RC" -eq 0

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
