#!/usr/bin/env bash
# test-chorus-werk-sync-auto-repair.sh — tests for #2846 auto-repair.
#
# Verifies that `chorus-werk-sync --auto-repair` (or env CHORUS_WERK_SYNC_AUTO_REPAIR=1):
#   1. On detached HEAD, retries with repair instead of aborting.
#   2. Emits canonical.sync.repaired so callers can distinguish from manual repair.
#   3. Without the flag, detached HEAD still aborts (no regression).
#
# Pattern mirrors test-chorus-werk-sync-2779.sh.

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
git clone -q "$REMOTE" "$CANONICAL"
git -C "$CANONICAL" config user.email "test@chorus.local"
git -C "$CANONICAL" config user.name "test"
git -C "$CANONICAL" checkout -q -b main 2>/dev/null || git -C "$CANONICAL" checkout -q main
echo "1" > "$CANONICAL/file.txt"
git -C "$CANONICAL" add file.txt
git -C "$CANONICAL" commit -q -m "1"
git -C "$CANONICAL" push -q origin main 2>/dev/null

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

# Case 1: --auto-repair on detached HEAD recovers and ff's
git -C "$CANONICAL" checkout -q --detach HEAD 2>/dev/null
: > "$SPINE_LOG"
OUTPUT=$(bash "$CHORUS_WERK_SYNC" --auto-repair 2>&1)
EXIT_CODE=$?
CURRENT_HEAD=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")
ORIGIN_MAIN=$(git -C "$CANONICAL" rev-parse origin/main 2>/dev/null)
LOCAL_MAIN=$(git -C "$CANONICAL" rev-parse main 2>/dev/null)
if [ "$EXIT_CODE" -eq 0 ] && [ "$CURRENT_HEAD" = "main" ] && [ "$LOCAL_MAIN" = "$ORIGIN_MAIN" ]; then
  echo "PASS: --auto-repair recovers detached HEAD and ff's to origin/main"
  PASS=$((PASS+1))
else
  echo "FAIL: --auto-repair did not recover (exit=$EXIT_CODE head=$CURRENT_HEAD)"
  echo "  output: $OUTPUT"
  FAIL=$((FAIL+1))
fi

# Case 2: spine emits canonical.sync.repaired
if grep -q "canonical.sync.repaired" "$SPINE_LOG"; then
  echo "PASS: canonical.sync.repaired emitted on auto-repair"
  PASS=$((PASS+1))
else
  echo "FAIL: canonical.sync.repaired not in spine log"
  echo "  spine: $(cat $SPINE_LOG)"
  FAIL=$((FAIL+1))
fi

# Case 3: env var also works
git -C "$CANONICAL" checkout -q --detach HEAD 2>/dev/null
: > "$SPINE_LOG"
OUTPUT=$(CHORUS_WERK_SYNC_AUTO_REPAIR=1 bash "$CHORUS_WERK_SYNC" 2>&1)
EXIT_CODE=$?
CURRENT_HEAD=$(git -C "$CANONICAL" symbolic-ref --short HEAD 2>/dev/null || echo "DETACHED")
if [ "$EXIT_CODE" -eq 0 ] && [ "$CURRENT_HEAD" = "main" ]; then
  echo "PASS: CHORUS_WERK_SYNC_AUTO_REPAIR=1 env enables auto-repair"
  PASS=$((PASS+1))
else
  echo "FAIL: env var did not enable auto-repair (exit=$EXIT_CODE head=$CURRENT_HEAD)"
  FAIL=$((FAIL+1))
fi

# Case 4: without flag, detached HEAD still aborts
git -C "$CANONICAL" checkout -q --detach HEAD 2>/dev/null
OUTPUT=$(bash "$CHORUS_WERK_SYNC" 2>&1)
EXIT_CODE=$?
if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | grep -q "detached"; then
  echo "PASS: bare sync still aborts on detached HEAD (no regression)"
  PASS=$((PASS+1))
else
  echo "FAIL: bare sync should abort on detached HEAD without flag (exit=$EXIT_CODE)"
  FAIL=$((FAIL+1))
fi

echo
echo "=== auto-repair tests: $PASS pass / $FAIL fail ==="
exit $((FAIL > 0 ? 1 : 0))
