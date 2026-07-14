#!/usr/bin/env bash
# test-chorus-werk-sweep.sh — hermetic tests for the git-hygiene sweeper (#3541).
#
# Every test brings its own world: a temp canonical repo + werk base + stubbed
# board (CHORUS_CARDS_BIN) and spine (CHORUS_LOG_BIN). No live board, no live
# spine, no real /chorus paths.
#
# Policy under test (the #3394 line):
#   card WIP/open            -> PRESERVE (report)
#   card closed + content ON main    -> remove (content-check passes)
#   card Won't Do + content NOT on main -> ABANDON (explicitly discarded work)
#   card Done + content NOT on main  -> FLAG, never auto-delete (#3394 false-Done shape)
#   detached-HEAD werk       -> FLAG
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SWEEP="$SCRIPT_DIR/chorus-werk-sweep"

PASS=0; FAIL=0
assert() {
  local label="$1"; shift
  if "$@"; then PASS=$((PASS+1)); echo "PASS: $label"
  else FAIL=$((FAIL+1)); echo "FAIL: $label"; fi
}

if [ ! -x "$SWEEP" ]; then
  echo "FAIL: chorus-werk-sweep not found/executable at $SWEEP"
  exit 1
fi

TEST_ROOT=$(mktemp -d)
CANONICAL="$TEST_ROOT/chorus"
WERK_BASE="$TEST_ROOT/chorus-werk"
STUBS="$TEST_ROOT/stubs"
mkdir -p "$CANONICAL" "$WERK_BASE" "$STUBS"
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

git -C "$CANONICAL" init -q -b main
git -C "$CANONICAL" config user.email t@t
git -C "$CANONICAL" config user.name t
echo canonical > "$CANONICAL/README.md"
git -C "$CANONICAL" add README.md && git -C "$CANONICAL" commit -q -m init
git -C "$CANONICAL" update-ref refs/remotes/origin/main "$(git -C "$CANONICAL" rev-parse main)"

export CHORUS_HOME="$CANONICAL"
export CHORUS_WERK_BASE="$WERK_BASE"
export SPINE_CAPTURE="$TEST_ROOT/spine.log"

# Board stub: status per card id, driven by a case table.
cat > "$STUBS/cards" <<'EOS'
#!/usr/bin/env bash
id="$2"
case "$id" in
  101) s="WIP" ;;
  102) s="Done" ;;
  103) s="Won't Do" ;;
  104) s="Done" ;;
  105) s="Next" ;;
  *)   exit 1 ;;
esac
echo "#$id stub"
echo "  Status:   $s"
EOS
cat > "$STUBS/chorus-log" <<'EOS'
#!/usr/bin/env bash
echo "$@" >> "${SPINE_CAPTURE:?}"
EOS
chmod +x "$STUBS/cards" "$STUBS/chorus-log"
export CHORUS_CARDS_BIN="$STUBS/cards"
export CHORUS_LOG_BIN="$STUBS/chorus-log"

CW="$SCRIPT_DIR/chorus-werk"
mkwerk() { "$CW" add "$1" "$2" >/dev/null 2>&1; }

# Fixture set:
mkwerk kade 101                     # WIP card -> preserve
mkwerk kade 102                     # Done, content on main (no commits) -> remove
mkwerk kade 103                     # Won't Do, unlanded commit -> abandon
echo dead > "$WERK_BASE/kade-103/dead.txt"
git -C "$WERK_BASE/kade-103" add dead.txt
git -C "$WERK_BASE/kade-103" -c user.email=t@t -c user.name=t commit -q -m "kade: never landing"
mkwerk kade 104                     # Done but content NOT on main (#3394 false-Done) -> FLAG
echo real > "$WERK_BASE/kade-104/real.txt"
git -C "$WERK_BASE/kade-104" add real.txt
git -C "$WERK_BASE/kade-104" -c user.email=t@t -c user.name=t commit -q -m "kade: real unlanded work"
mkwerk kade 105                     # Next (open) -> preserve
# detached-HEAD werk on a WIP card -> flagged, preserved
git -C "$WERK_BASE/kade-101" checkout -q --detach HEAD

OUT=$("$SWEEP" 2>&1)
RC=$?

assert "sweeper exits 0 on a mixed field" test "$RC" -eq 0

# preserve
assert "WIP werk preserved" test -d "$WERK_BASE/kade-101"
assert "Next werk preserved" test -d "$WERK_BASE/kade-105"
assert "report names preserved cards" bash -c 'grep -q "preserve" <<< "$1" && grep -q "101" <<< "$1"' _ "$OUT"

# remove (content on main)
assert "Done+landed werk removed" test ! -d "$WERK_BASE/kade-102"
assert "Done+landed branch removed" test -z "$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/102 2>/dev/null)"

# abandon (Won't Do + unlanded content)
assert "WontDo+unlanded werk abandoned" test ! -d "$WERK_BASE/kade-103"
assert "WontDo+unlanded branch gone" test -z "$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/103 2>/dev/null)"
assert "abandon witnessed via werk.abandoned" bash -c 'grep -q "werk.abandoned" "$SPINE_CAPTURE" && grep -q "card_id=103" "$SPINE_CAPTURE"'

# flag, never delete the WORK (#3394 false-Done). Note: chorus-werk remove takes
# the worktree DIR before the merged-ness proof refuses at the branch step, so the
# dir may be gone — the invariant is the BRANCH (the actual commits) survives and
# the contradiction is flagged for a human, never silently resolved.
assert "false-Done branch survives (the work is never lost)" test -n "$(git -C "$CANONICAL" rev-parse --verify refs/heads/kade/104 2>/dev/null)"
assert "false-Done unlanded commit still reachable" bash -c 'git -C "'"$CANONICAL"'" log refs/heads/kade/104 --format=%s | grep -q "real unlanded work"' 
assert "report flags the false-Done contradiction" bash -c 'grep -qi "flag" <<< "$1" && grep -q "104" <<< "$1"' _ "$OUT"

# detached-HEAD detection
assert "detached-HEAD werk flagged in report" bash -c 'grep -qi "detached" <<< "$1" && grep -q "101" <<< "$1"' _ "$OUT"

# report + witness
assert "summary line reports swept/abandoned/flagged/preserved counts" \
  bash -c 'grep -qE "swept=1 .*abandoned=1 .*flagged=[12] .*preserved=2" <<< "$1"' _ "$OUT"
assert "sweep witnessed via werk.sweep.completed" bash -c 'grep -q "werk.sweep.completed" "$SPINE_CAPTURE"'

# fail-closed board: unknown card -> preserve + flag, never delete
mkwerk kade 999
OUT2=$("$SWEEP" 2>&1)
assert "unknown-card werk preserved (fail-closed board)" test -d "$WERK_BASE/kade-999"
assert "unknown-card flagged in report" bash -c 'grep -q "999" <<< "$1"' _ "$OUT2"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
