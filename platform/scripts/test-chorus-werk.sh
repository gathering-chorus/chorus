#!/usr/bin/env bash
# test-chorus-werk.sh — tests for chorus-werk ephemeral-worktree script (#2913).
#
# #2913 rewrite: chorus-werk moved from a persistent-per-role model
# (init/repoint/pull/close around one stable chorus-werk/<role>/ dir) to an
# ephemeral-per-card model (add/remove around chorus-werk/<role>-<card>/ dirs
# created on pull, destroyed on acp). No repoint — no branch-swap-in-place —
# so the detached-HEAD failure class cannot occur.
#
# Surface under test:
#   add <role> <card-id>     create chorus-werk/<role>-<card>/ on branch
#                            <role>/<card-id> from origin/main. Idempotent.
#   remove <role> <card-id>  git worktree remove + branch delete + remote
#                            cleanup + git worktree prune. Idempotent.
#                            Refuses on dirty werk.
#   status                   list ephemeral worktrees (chorus-werk/<role>-*).
#
# Runs against a temp git repo so nothing touches the real /chorus or
# /chorus-werk paths. Run directly (not via hook-intercepted Bash) since
# setup uses raw git, not git-queue.sh.

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
  if [ -d "$CANONICAL/.git" ]; then
    for wt in "$WERK_BASE"/*/; do
      [ -d "$wt" ] || continue
      git -C "$CANONICAL" worktree remove --force "$wt" 2>/dev/null || true
    done
  fi
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

# Fake canonical git repo
git -C "$CANONICAL" init -q -b main
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
echo "canonical" > "$CANONICAL/README.md"
git -C "$CANONICAL" add README.md
git -C "$CANONICAL" commit -q -m "init"
# origin/main present — production always has it; add prefers it.
git -C "$CANONICAL" update-ref refs/remotes/origin/main "$(git -C "$CANONICAL" rev-parse main)"

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

# --- TEST 1: add creates an ephemeral worktree at <role>-<card> ---
"$CHORUS_WERK" add kade 2913 > /dev/null 2>&1
assert "add creates ephemeral werk dir chorus-werk/kade-2913" test -d "$WERK_BASE/kade-2913"
assert "add creates .git pointer" test -f "$WERK_BASE/kade-2913/.git"

# --- TEST 2: add lands the worktree on branch <role>/<card-id> ---
WT_BRANCH=$(git -C "$WERK_BASE/kade-2913" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "add lands worktree on branch kade/2913" test "$WT_BRANCH" = "kade/2913"

# --- TEST 3: add is idempotent (re-add same card -> no-op, exit 0) ---
"$CHORUS_WERK" add kade 2913 > /dev/null 2>&1
RC=$?
assert "add idempotent (exit 0 on existing)" test "$RC" -eq 0
WT_BRANCH=$(git -C "$WERK_BASE/kade-2913" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "werk still on kade/2913 after second add" test "$WT_BRANCH" = "kade/2913"

# --- TEST 4: add bakes per-werk git identity ---
WERK_EMAIL=$(git -C "$WERK_BASE/kade-2913" config --local user.email 2>/dev/null)
WERK_NAME=$(git -C "$WERK_BASE/kade-2913" config --local user.name 2>/dev/null)
assert "add bakes kade@chorus.local identity" test "$WERK_EMAIL" = "kade@chorus.local"
assert "add bakes kade as user.name" test "$WERK_NAME" = "kade"

# --- TEST 5: status lists the ephemeral worktree ---
STATUS_OUT=$("$CHORUS_WERK" status 2>&1)
assert "status mentions kade-2913" echo "$STATUS_OUT" | grep -q "kade-2913"
assert "status mentions the branch" echo "$STATUS_OUT" | grep -q "kade/2913"

# --- TEST 6: >1 card per role -> two isolated worktrees ---
# The use case the persistent model could not do. add for a second card must
# NOT touch the first card's worktree.
"$CHORUS_WERK" add kade 2914 > /dev/null 2>&1
RC=$?
assert "add second card exits 0" test "$RC" -eq 0
assert "second card gets its own worktree dir" test -d "$WERK_BASE/kade-2914"
WT1_BRANCH=$(git -C "$WERK_BASE/kade-2913" rev-parse --abbrev-ref HEAD 2>/dev/null)
WT2_BRANCH=$(git -C "$WERK_BASE/kade-2914" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "first card worktree untouched (still kade/2913)" test "$WT1_BRANCH" = "kade/2913"
assert "second card worktree on kade/2914" test "$WT2_BRANCH" = "kade/2914"

# --- TEST 7: remove tears down one card's worktree, leaves the other ---
"$CHORUS_WERK" remove kade 2914 > /dev/null 2>&1
RC=$?
assert "remove exits 0 on clean werk" test "$RC" -eq 0
assert "remove deletes the worktree dir" test ! -d "$WERK_BASE/kade-2914"
assert "remove deletes the local branch" test -z "$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/2914 2>/dev/null)"
assert "remove left the other card's worktree intact" test -d "$WERK_BASE/kade-2913"

# --- TEST 8: remove prunes stale .git/worktrees admin entry (Silas 3b) ---
STALE=$(git -C "$CANONICAL" worktree list --porcelain 2>/dev/null | grep -c "kade-2914")
assert "remove prunes stale .git/worktrees admin entry" test "$STALE" -eq 0

# --- TEST 9: remove is idempotent ---
"$CHORUS_WERK" remove kade 2914 > /dev/null 2>&1
RC=$?
assert "remove on already-removed card exits 0" test "$RC" -eq 0

# --- TEST 10: remove refuses on dirty werk — never silently lose work ---
echo "uncommitted" > "$WERK_BASE/kade-2913/dirty-file.txt"
"$CHORUS_WERK" remove kade 2913 > /dev/null 2>&1
RC=$?
assert "remove refuses on uncommitted file" test "$RC" -ne 0
assert "worktree preserved when remove refused" test -d "$WERK_BASE/kade-2913"
assert "branch preserved when remove refused" test -n "$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/2913 2>/dev/null)"
rm -f "$WERK_BASE/kade-2913/dirty-file.txt"

"$CHORUS_WERK" remove kade 2913 > /dev/null 2>&1
RC=$?
assert "remove succeeds once werk is clean again" test "$RC" -eq 0
assert "worktree gone after clean remove" test ! -d "$WERK_BASE/kade-2913"

# --- TEST 11: canonical's HEAD never moved throughout ---
CANON_BRANCH=$(git -C "$CANONICAL" rev-parse --abbrev-ref HEAD 2>/dev/null)
assert "canonical HEAD stayed on main throughout" test "$CANON_BRANCH" = "main"

# --- TEST 12: no-args / unknown subcommand / missing args exit non-zero ---
"$CHORUS_WERK" > /dev/null 2>&1
assert "no-args exits non-zero" test "$?" -ne 0
"$CHORUS_WERK" frobnicate kade 1 > /dev/null 2>&1
assert "unknown subcommand exits non-zero" test "$?" -ne 0
"$CHORUS_WERK" add kade > /dev/null 2>&1
assert "add without card-id exits non-zero" test "$?" -ne 0
"$CHORUS_WERK" add > /dev/null 2>&1
assert "add without role exits non-zero" test "$?" -ne 0
"$CHORUS_WERK" remove kade > /dev/null 2>&1
assert "remove without card-id exits non-zero" test "$?" -ne 0
"$CHORUS_WERK" add notarole 1 > /dev/null 2>&1
assert "add with unknown role exits non-zero" test "$?" -ne 0

# --- TEST 13: add bases the new branch on origin/main, not stale local main ---
LOCAL_MAIN=$(git -C "$CANONICAL" rev-parse main 2>/dev/null)
TREE=$(git -C "$CANONICAL" rev-parse HEAD^{tree})
FUTURE=$(echo "peer push" | git -C "$CANONICAL" commit-tree "$TREE" -p "$LOCAL_MAIN" -m "peer push")
git -C "$CANONICAL" update-ref refs/remotes/origin/main "$FUTURE"
assert "test-13 setup: origin/main ahead of local main" test "$FUTURE" != "$LOCAL_MAIN"

"$CHORUS_WERK" add kade 3001 > /dev/null 2>&1
WT_BASE=$(git -C "$WERK_BASE/kade-3001" rev-parse "kade/3001" 2>/dev/null)
assert "add bases new branch on origin/main tip" test "$WT_BASE" = "$FUTURE"
"$CHORUS_WERK" remove kade 3001 > /dev/null 2>&1
git -C "$CANONICAL" update-ref refs/remotes/origin/main "$(git -C "$CANONICAL" rev-parse main)"

# --- TEST 14: add bootstraps node_modules symlinks from canonical ---
printf 'node_modules\nnode_modules/\n' > "$CANONICAL/.gitignore"
mkdir -p "$CANONICAL/platform/api" "$CANONICAL/platform/empty-pkg"
echo '{"name":"api"}' > "$CANONICAL/platform/api/package.json"
echo '{"name":"empty"}' > "$CANONICAL/platform/empty-pkg/package.json"
git -C "$CANONICAL" add -A >/dev/null 2>&1
git -C "$CANONICAL" commit -q -m "fixture: npm projects" >/dev/null 2>&1
git -C "$CANONICAL" update-ref refs/remotes/origin/main "$(git -C "$CANONICAL" rev-parse main)"
# node_modules created AFTER commit (gitignored — bootstrap is what gets them
# into the werk, not worktree-add).
mkdir -p "$CANONICAL/platform/api/node_modules/some-pkg"
echo '{}' > "$CANONICAL/platform/api/node_modules/some-pkg/package.json"

"$CHORUS_WERK" add kade 3002 > /dev/null 2>&1
assert "add creates platform/api/node_modules in werk" test -e "$WERK_BASE/kade-3002/platform/api/node_modules"
assert "platform/api/node_modules is a symlink" test -L "$WERK_BASE/kade-3002/platform/api/node_modules"
LINK_TARGET=$(readlink "$WERK_BASE/kade-3002/platform/api/node_modules" 2>/dev/null)
assert "node_modules symlink points at canonical" test "$LINK_TARGET" = "$CANONICAL/platform/api/node_modules"
assert "node_modules content reachable through symlink" test -f "$WERK_BASE/kade-3002/platform/api/node_modules/some-pkg/package.json"
assert "no symlink for npm dir without canonical node_modules" test ! -e "$WERK_BASE/kade-3002/platform/empty-pkg/node_modules"
"$CHORUS_WERK" remove kade 3002 > /dev/null 2>&1

# --- TEST 15: remove emits card.branch.closed spine event ---
mkdir -p "$CANONICAL/platform/scripts"
SPINE_LOG="$TEST_ROOT/spine.log"
cat > "$CANONICAL/platform/scripts/chorus-log" <<EOF
#!/usr/bin/env bash
echo "\$@" >> "$SPINE_LOG"
EOF
chmod +x "$CANONICAL/platform/scripts/chorus-log"

"$CHORUS_WERK" add kade 3003 > /dev/null 2>&1
"$CHORUS_WERK" remove kade 3003 > /dev/null 2>&1
RC=$?
assert "test-15 remove exits 0" test "$RC" -eq 0
assert "remove emits card.branch.closed spine event" grep -q "card.branch.closed" "$SPINE_LOG"
CLOSED_LINE=$(grep "card.branch.closed" "$SPINE_LOG" | head -1)
assert "card.branch.closed names role" grep -q "kade" <<< "$CLOSED_LINE"
assert "card.branch.closed names card" grep -q "card=3003" <<< "$CLOSED_LINE"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
