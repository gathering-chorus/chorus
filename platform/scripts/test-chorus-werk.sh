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

# --- TEST 12: init bakes per-werk git identity (Wren #2735 review) ---
"$CHORUS_WERK" init wren > /dev/null 2>&1
WREN_EMAIL=$(git -C "$WERK_BASE/wren" config --local user.email 2>/dev/null)
WREN_NAME=$(git -C "$WERK_BASE/wren" config --local user.name 2>/dev/null)
assert "wren werk has wren@chorus.local" test "$WREN_EMAIL" = "wren@chorus.local"
assert "wren werk has wren as user.name" test "$WREN_NAME" = "wren"
"$CHORUS_WERK" init silas > /dev/null 2>&1
SILAS_EMAIL=$(git -C "$WERK_BASE/silas" config --local user.email 2>/dev/null)
assert "silas werk has silas@chorus.local" test "$SILAS_EMAIL" = "silas@chorus.local"

# --- TEST 13: close — branch closes when card closes (#2740) ---
# Setup: kade werk repointed to a card branch, simulate post-acp state.
"$CHORUS_WERK" pull kade 4001 > /dev/null 2>&1
WT_BRANCH=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "test-13 setup: kade werk on kade/4001" test "$WT_BRANCH" = "kade/4001"

# Close — uses --no-done-check since the test fixture has no live cards CLI.
# Production /acp wiring guarantees Done state via call order; the flag exists
# for tests + manual cleanup paths where the caller takes responsibility.
"$CHORUS_WERK" close --no-done-check kade 4001 > /dev/null 2>&1
RC=$?
assert "close exits 0 on clean werk" test "$RC" -eq 0

# After close: worktree detached at main's tip, local branch deleted
WT_HEAD_TYPE=$(git -C "$WERK_BASE/kade" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "close detaches werk HEAD" test "$WT_HEAD_TYPE" = "HEAD"
WT_TIP=$(git -C "$WERK_BASE/kade" rev-parse HEAD 2>/dev/null)
MAIN_TIP=$(git -C "$CANONICAL" rev-parse main 2>/dev/null)
assert "close lands werk at main's tip" test "$WT_TIP" = "$MAIN_TIP"

# Local branch ref should be gone
LOCAL_BRANCH_EXISTS=$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/4001 2>/dev/null)
assert "close deletes local branch" test -z "$LOCAL_BRANCH_EXISTS"

# Canonical HEAD never moved
CANON_BRANCH=$(git -C "$CANONICAL" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "canonical HEAD still on main after close" test "$CANON_BRANCH" = "main"

# --- TEST 14: close is idempotent ---
"$CHORUS_WERK" close --no-done-check kade 4001 > /dev/null 2>&1
RC=$?
assert "close on already-closed branch exits 0" test "$RC" -eq 0

# --- TEST 15: close refuses on dirty werk ---
"$CHORUS_WERK" pull kade 4002 > /dev/null 2>&1
echo "uncommitted" > "$WERK_BASE/kade/dirty-file.txt"
"$CHORUS_WERK" close --no-done-check kade 4002 > /dev/null 2>&1
RC=$?
assert "close refuses on uncommitted file" test "$RC" -ne 0
LOCAL_BRANCH_EXISTS=$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/4002 2>/dev/null)
assert "branch preserved when close refused" test -n "$LOCAL_BRANCH_EXISTS"
rm -f "$WERK_BASE/kade/dirty-file.txt"

# Cleanup: close 4002 properly now that werk is clean
"$CHORUS_WERK" close --no-done-check kade 4002 > /dev/null 2>&1

# --- TEST 16: close requires <role> and <card-id> ---
"$CHORUS_WERK" close --no-done-check > /dev/null 2>&1
RC=$?
assert "close without role exits non-zero" test "$RC" -ne 0

"$CHORUS_WERK" close --no-done-check kade > /dev/null 2>&1
RC=$?
assert "close without card-id exits non-zero" test "$RC" -ne 0

# --- TEST 17: init bootstraps node_modules from canonical (#2758) ---
# Werks accumulate a 30s npm-install penalty per init because they're
# created without node_modules. Symlink from canonical's matching npm
# dirs so the werk is immediately ready for tsc/jest/pre-commit.
# Real canonical gitignores node_modules; mirror that in the fixture so
# `git worktree add` doesn't check them out as real dirs and shadow the
# symlink. Commit only the package.json files + .gitignore.
echo 'node_modules/' > "$CANONICAL/.gitignore"
mkdir -p "$CANONICAL/platform/api" "$CANONICAL/platform/tests" "$CANONICAL/platform/empty-pkg"
echo '{"name":"api"}' > "$CANONICAL/platform/api/package.json"
echo '{"name":"tests"}' > "$CANONICAL/platform/tests/package.json"
echo '{"name":"empty"}' > "$CANONICAL/platform/empty-pkg/package.json"
git -C "$CANONICAL" add -A >/dev/null 2>&1
git -C "$CANONICAL" commit -q -m "fixture: npm projects" >/dev/null 2>&1
# Add untracked node_modules AFTER the commit (gitignored, won't propagate
# to werk via worktree-add — bootstrap is what gets them there).
mkdir -p "$CANONICAL/platform/api/node_modules/some-pkg"
echo '{}' > "$CANONICAL/platform/api/node_modules/some-pkg/package.json"
mkdir -p "$CANONICAL/platform/tests/node_modules/another-pkg"
echo '{}' > "$CANONICAL/platform/tests/node_modules/another-pkg/package.json"

"$CHORUS_WERK" remove kade > /dev/null 2>&1
"$CHORUS_WERK" init kade > /dev/null 2>&1

assert "init creates platform/api/node_modules in werk" test -e "$WERK_BASE/kade/platform/api/node_modules"
assert "platform/api/node_modules is a symlink" test -L "$WERK_BASE/kade/platform/api/node_modules"
LINK_TARGET=$(readlink "$WERK_BASE/kade/platform/api/node_modules" 2>/dev/null)
assert "platform/api/node_modules points at canonical" test "$LINK_TARGET" = "$CANONICAL/platform/api/node_modules"
assert "platform/api/node_modules content reachable" test -f "$WERK_BASE/kade/platform/api/node_modules/some-pkg/package.json"

assert "init creates platform/tests/node_modules in werk" test -e "$WERK_BASE/kade/platform/tests/node_modules"
assert "platform/tests/node_modules is a symlink" test -L "$WERK_BASE/kade/platform/tests/node_modules"

# Skip case: canonical has package.json but no node_modules → werk has no symlink
assert "no symlink for npm dir without canonical node_modules" test ! -e "$WERK_BASE/kade/platform/empty-pkg/node_modules"

# Idempotency: re-init should not error and symlinks remain
"$CHORUS_WERK" init kade > /dev/null 2>&1
RC=$?
assert "init idempotent after bootstrap" test "$RC" -eq 0
assert "symlink still present after re-init" test -L "$WERK_BASE/kade/platform/api/node_modules"

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
