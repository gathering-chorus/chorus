#!/usr/bin/env bash
# test-chorus-werk-parallel.sh — parallel-roles integration test (#2735).
#
# Demonstrates the structural fix: two simulated roles (kade + wren) work
# in parallel werks against a shared canonical. Each commits to their own
# branch in their own worktree; canonical's HEAD stays on main throughout;
# neither role's HEAD contaminates the other.
#
# This is the test that would have caught the 2026-05-05 incident
# (canonical sitting on wren/2731 while Kade tried to ship #2733). With
# per-role werks, the cross-role guard becomes vestigial because the
# wrong-role-on-wrong-branch state cannot occur.
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_WERK="$SCRIPT_DIR/chorus-werk"

PASS=0
FAIL=0

if [ ! -x "$CHORUS_WERK" ]; then
  echo "FAIL: chorus-werk script not found"
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

# --- Setup: bare remote + canonical with one commit on main ------------------
git init -q --bare "$REMOTE"
git clone -q "$REMOTE" "$CANONICAL"
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
git -C "$CANONICAL" checkout -q -b main 2>/dev/null || git -C "$CANONICAL" checkout -q main
echo "shared" > "$CANONICAL/shared.txt"
git -C "$CANONICAL" add shared.txt
git -C "$CANONICAL" commit -q -m "init"
git -C "$CANONICAL" push -q origin main

CANON_INITIAL_HEAD=$(git -C "$CANONICAL" rev-parse HEAD)

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

# Negation helpers — `!` doesn't pass through assert cleanly.
contains() { echo "$1" | grep -q "$2"; }
not_contains() { ! echo "$1" | grep -q "$2"; }
not() { ! "$@"; }

# --- Pull both roles in parallel to their own card branches ------------------
"$CHORUS_WERK" pull kade 9001 > /dev/null 2>&1
"$CHORUS_WERK" pull wren 9002 > /dev/null 2>&1

assert "kade werk exists" test -d "$WERK_BASE/kade"
assert "wren werk exists" test -d "$WERK_BASE/wren"

KADE_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD)
WREN_BRANCH=$(git -C "$WERK_BASE/wren" rev-parse --abbrev-ref HEAD)
assert "kade on its own branch" test "$KADE_BRANCH" = "kade/9001"
assert "wren on its own branch" test "$WREN_BRANCH" = "wren/9002"

# --- Each role configures git identity in its own werk -----------------------
git -C "$WERK_BASE/kade" config user.email "kade@chorus.local"
git -C "$WERK_BASE/kade" config user.name "kade"
git -C "$WERK_BASE/wren" config user.email "wren@chorus.local"
git -C "$WERK_BASE/wren" config user.name "wren"

# --- Each role makes a commit in their own werk ------------------------------
echo "kade-edit" > "$WERK_BASE/kade/kade.txt"
git -C "$WERK_BASE/kade" add kade.txt
git -C "$WERK_BASE/kade" commit -q -m "kade: edit"

echo "wren-edit" > "$WERK_BASE/wren/wren.txt"
git -C "$WERK_BASE/wren" add wren.txt
git -C "$WERK_BASE/wren" commit -q -m "wren: edit"

# --- Each role pushes their branch ------------------------------------------
git -C "$WERK_BASE/kade" push -q origin kade/9001 2>/dev/null
git -C "$WERK_BASE/wren" push -q origin wren/9002 2>/dev/null

# --- Verify isolation: canonical's HEAD never moved off its initial position --
CANON_FINAL_HEAD=$(git -C "$CANONICAL" rev-parse HEAD)
CANON_FINAL_BRANCH=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "?")
assert "canonical HEAD stayed at initial commit" test "$CANON_FINAL_HEAD" = "$CANON_INITIAL_HEAD"
assert "canonical still on main branch" test "$CANON_FINAL_BRANCH" = "main"

# --- Verify each role's branch contains only its own commit ------------------
KADE_LOG=$(git -C "$REMOTE" log --format=%s kade/9001 2>/dev/null)
WREN_LOG=$(git -C "$REMOTE" log --format=%s wren/9002 2>/dev/null)
assert "kade branch contains kade's commit" contains "$KADE_LOG" "kade: edit"
assert "kade branch does NOT contain wren's commit" not_contains "$KADE_LOG" "wren: edit"
assert "wren branch contains wren's commit" contains "$WREN_LOG" "wren: edit"
assert "wren branch does NOT contain kade's commit" not_contains "$WREN_LOG" "kade: edit"

# --- Verify each werk only sees its own file ---------------------------------
assert "kade werk has kade.txt" test -f "$WERK_BASE/kade/kade.txt"
assert "kade werk does NOT have wren.txt" test ! -f "$WERK_BASE/kade/wren.txt"
assert "wren werk has wren.txt" test -f "$WERK_BASE/wren/wren.txt"
assert "wren werk does NOT have kade.txt" test ! -f "$WERK_BASE/wren/kade.txt"

# --- Repoint kade's werk to a new card without disturbing wren ---------------
"$CHORUS_WERK" pull kade 9003 > /dev/null 2>&1
KADE_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD)
WREN_BRANCH=$(git -C "$WERK_BASE/wren" rev-parse --abbrev-ref HEAD)
assert "kade repointed to new card" test "$KADE_BRANCH" = "kade/9003"
assert "wren branch unaffected by kade's repoint" test "$WREN_BRANCH" = "wren/9002"

# --- Canonical's HEAD STILL on main after all that --------------------------
CANON_FINAL_BRANCH=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "?")
assert "canonical still on main after parallel work" test "$CANON_FINAL_BRANCH" = "main"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
