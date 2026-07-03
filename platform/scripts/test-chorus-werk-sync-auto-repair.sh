#!/usr/bin/env bash
# test-chorus-werk-sync-auto-repair.sh — tests for the `repair` subcommand
# (detached-HEAD recovery, #2779). #3033: rewritten off the retired #2846
# `--auto-repair` flag / CHORUS_WERK_SYNC_AUTO_REPAIR env interface, which no
# longer exists — chorus-werk-sync exposes `repair` / `recover` subcommands now.
#
# Verifies `chorus-werk-sync repair`:
#   1. On detached HEAD, re-attaches HEAD to main and ff's to origin/main.
#   2. Emits canonical.repaired.
#   3. Is idempotent — a clean no-op when already on main.
#   4. Bare sync (no subcommand) still aborts on detached HEAD (no regression).

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
SPINE_LOG="$TEST_ROOT/spine.log"
trap 'rm -rf "$TEST_ROOT"' EXIT

git init -q --bare "$REMOTE"
# #3033: init canonical directly on main + add remote, instead of cloning the
# empty bare repo (whose HEAD follows init.defaultBranch and left the first
# commit off main, so `push origin main` failed with "src refspec main").
git init -q "$CANONICAL"
git -C "$CANONICAL" symbolic-ref HEAD refs/heads/main
git -C "$CANONICAL" remote add origin "$REMOTE"
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
echo "1" > "$CANONICAL/file.txt"
git -C "$CANONICAL" add file.txt
git -C "$CANONICAL" commit -q -m "1"
git -C "$CANONICAL" push -q -u origin main
# point the bare remote's HEAD at main so clones (the peer below) land on main,
# not the host default branch — otherwise the peer commits off-main and its
# `push origin main` fails, leaving origin/main un-advanced.
git -C "$REMOTE" symbolic-ref HEAD refs/heads/main

# Peer ahead so repair has something to ff to.
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

STUB_DIR="$TEST_ROOT/stub-bin"
mkdir -p "$STUB_DIR"
cat > "$STUB_DIR/chorus-log" <<EOF
#!/usr/bin/env bash
echo "\$@" >> "$SPINE_LOG"
EOF
chmod +x "$STUB_DIR/chorus-log"
export PATH="$STUB_DIR:$PATH"
# #3606: the Rust werk-sync emits via CHORUS_LOG_BIN override (absolute-path
# emit otherwise — #3151); export it so the stub captures emits hermetically.
export CHORUS_LOG_BIN="$STUB_DIR/chorus-log"

# Case 1: `repair` on detached HEAD re-attaches to main and ff's to origin/main
git -C "$CANONICAL" checkout -q --detach HEAD 2>/dev/null
: > "$SPINE_LOG"
OUTPUT=$(bash "$CHORUS_WERK_SYNC" repair 2>&1)
EXIT_CODE=$?
CURRENT_HEAD=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")
ORIGIN_MAIN=$(git -C "$CANONICAL" rev-parse origin/main 2>/dev/null)
LOCAL_MAIN=$(git -C "$CANONICAL" rev-parse main 2>/dev/null)
if [ "$EXIT_CODE" -eq 0 ] && [ "$CURRENT_HEAD" = "main" ] && [ "$LOCAL_MAIN" = "$ORIGIN_MAIN" ]; then
  echo "PASS: repair recovers detached HEAD and ff's to origin/main"
  PASS=$((PASS+1))
else
  echo "FAIL: repair did not recover (exit=$EXIT_CODE head=$CURRENT_HEAD)"
  echo "  output: $OUTPUT"
  FAIL=$((FAIL+1))
fi

# Case 2: spine emits canonical.repaired
if grep -q "canonical.repaired" "$SPINE_LOG"; then
  echo "PASS: canonical.repaired emitted on repair"
  PASS=$((PASS+1))
else
  echo "FAIL: canonical.repaired not in spine log"
  echo "  spine: $(cat "$SPINE_LOG")"
  FAIL=$((FAIL+1))
fi

# Case 3: repair is idempotent — a clean no-op when already on main
: > "$SPINE_LOG"
OUTPUT=$(bash "$CHORUS_WERK_SYNC" repair 2>&1)
EXIT_CODE=$?
CURRENT_HEAD=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")
if [ "$EXIT_CODE" -eq 0 ] && [ "$CURRENT_HEAD" = "main" ]; then
  echo "PASS: repair is idempotent (no-op when already on main)"
  PASS=$((PASS+1))
else
  echo "FAIL: repair on already-main should be a clean no-op (exit=$EXIT_CODE head=$CURRENT_HEAD)"
  echo "  output: $OUTPUT"
  FAIL=$((FAIL+1))
fi

# Case 4: bare sync (no subcommand) still aborts on detached HEAD (no regression)
git -C "$CANONICAL" checkout -q --detach HEAD 2>/dev/null
OUTPUT=$(bash "$CHORUS_WERK_SYNC" 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | grep -q "detached"; then
  echo "PASS: bare sync still aborts on detached HEAD (no regression)"
  PASS=$((PASS+1))
else
  echo "FAIL: bare sync should abort on detached HEAD (exit=$EXIT_CODE)"
  echo "  output: $OUTPUT"
  FAIL=$((FAIL+1))
fi

echo
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $((FAIL > 0 ? 1 : 0))
