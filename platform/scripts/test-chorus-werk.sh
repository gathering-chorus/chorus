#!/usr/bin/env bash
# test-chorus-werk.sh — tests for chorus-werk bootstrap script (#2735).
#
# Covers: init creates a worktree, init is idempotent, repoint switches branch,
# status lists state, remove tears down. Runs against a temp git repo so
# nothing touches the real /chorus or /chorus-werk paths.
#
# Run directly (not via Claude hook-intercepted Bash) since setup uses raw
# git, not git-queue.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_WERK="$SCRIPT_DIR/chorus-werk"

PASS=0
FAIL=0

if [ ! -x "$CHORUS_WERK" ]; then
  echo "FAIL: chorus-werk script not found or not executable at $CHORUS_WERK"
  exit 1
fi

TEST_ROOT=$(mktemp -d)
CANONICAL="$TEST_ROOT/chorus"
WERK_BASE="$TEST_ROOT/chorus-werk"
mkdir -p "$CANONICAL"

cleanup() {
  # Remove worktrees first (git refuses to delete locked branches otherwise)
  if [ -d "$CANONICAL/.git" ]; then
    for wt in "$WERK_BASE"/*/; do
      [ -d "$wt" ] || continue
      git -C "$CANONICAL" worktree remove --force "$wt" 2>/dev/null || true
    done
  fi
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

# Set up a fake canonical git repo
git -C "$CANONICAL" init -q -b main
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
echo "canonical" > "$CANONICAL/README.md"
git -C "$CANONICAL" add README.md
git -C "$CANONICAL" commit -q -m "init"

# Override env so chorus-werk targets the temp repo
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

# --- TEST 1: init creates a worktree at main's tip in detached state ---
"$CHORUS_WERK" init kade > /dev/null 2>&1
assert "init creates werk dir" test -d "$WERK_BASE/kade"
assert "init creates .git pointer" test -f "$WERK_BASE/kade/.git"
WT_HEAD_TYPE=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "init leaves worktree detached" test "$WT_HEAD_TYPE" = "HEAD"
WT_TIP=$(git -C "$WERK_BASE/kade" rev-parse HEAD 2>/dev/null)
MAIN_TIP=$(git -C "$CANONICAL" rev-parse main 2>/dev/null)
assert "init worktree at main's tip" test "$WT_TIP" = "$MAIN_TIP"

# --- TEST 2: init is idempotent ---
"$CHORUS_WERK" init kade > /dev/null 2>&1
RC=$?
assert "init idempotent (exit 0 on existing)" test "$RC" -eq 0
assert "werk dir still exists after second init" test -d "$WERK_BASE/kade"

# --- TEST 3: repoint switches the worktree to a new branch ---
"$CHORUS_WERK" repoint kade kade/test-card > /dev/null 2>&1
WT_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "repoint sets new branch" test "$WT_BRANCH" = "kade/test-card"

# --- TEST 4: status reports the worktree ---
STATUS_OUT=$("$CHORUS_WERK" status 2>&1)
assert "status mentions kade" echo "$STATUS_OUT" | grep -q "kade"
assert "status mentions current branch" echo "$STATUS_OUT" | grep -q "kade/test-card"

# --- TEST 5: remove tears down cleanly ---
"$CHORUS_WERK" remove kade > /dev/null 2>&1
assert "remove deletes werk dir" test ! -d "$WERK_BASE/kade"

# --- TEST 6: canonical's HEAD never moved ---
CANON_BRANCH=$(git -C "$CANONICAL" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "canonical HEAD stayed on main throughout" test "$CANON_BRANCH" = "main"

# --- TEST 7: usage / no-args exits non-zero with help ---
"$CHORUS_WERK" > /dev/null 2>&1
RC=$?
assert "no-args exits non-zero" test "$RC" -ne 0

# --- TEST 8: unknown role / unknown subcommand handled ---
"$CHORUS_WERK" frobnicate kade > /dev/null 2>&1
RC=$?
assert "unknown subcommand exits non-zero" test "$RC" -ne 0

# --- TEST 9: pull from clean state inits + repoints in one call ---
# Re-init for this test (previous remove tore down kade)
"$CHORUS_WERK" pull kade 2999 > /dev/null 2>&1
RC=$?
assert "pull exits 0 from uninited state" test "$RC" -eq 0
assert "pull creates werk dir" test -d "$WERK_BASE/kade"
WT_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "pull lands worktree on kade/<card-id>" test "$WT_BRANCH" = "kade/2999"

# --- TEST 10: pull on already-inited werk repoints to a new card ---
"$CHORUS_WERK" pull kade 3000 > /dev/null 2>&1
RC=$?
assert "pull exits 0 when werk already exists" test "$RC" -eq 0
WT_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "pull repoints existing werk to new card branch" test "$WT_BRANCH" = "kade/3000"

# --- TEST 11: pull missing card-id exits non-zero ---
"$CHORUS_WERK" pull kade > /dev/null 2>&1
RC=$?
assert "pull without card-id exits non-zero" test "$RC" -ne 0

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
