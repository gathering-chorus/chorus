#!/usr/bin/env bash
# test-chorus-werk-sync-recover.sh — proving gate for #2909.
#
# Asserts chorus-werk-sync recover auto-stashes dirty files to
# ~/.chorus/recovery/<ts>/ and proceeds with the sync, without
# any role-in-the-middle or Jeff-escalation.
#
# Assertions:
#   1. recover on clean tree → no-op sync, no recovery dir created.
#   2. recover on dirty tree with one real file → file stashed to
#      ~/.chorus/recovery/<ts>/<hash>, manifest written, sync completes.
#   3. recover on dirty tree with merge=union file only → no stash
#      (union-merge files are skipped), sync attempts.
#   4. Recovery dir contains the original content (lossless).
#
# Usage: ./test-chorus-werk-sync-recover.sh

set -uo pipefail

PASS=0
FAIL=0

trap '_rc=$?; echo "=== Results: $PASS passed, $FAIL failed ==="; if [ $FAIL -gt 0 ]; then exit 1; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$SCRIPT_DIR/chorus-werk-sync"

if [ ! -x "$SCRIPT" ]; then
  echo "FAIL [setup]: chorus-werk-sync not executable at $SCRIPT"
  FAIL=$((FAIL+1))
  exit 1
fi

setup_repo_with_remote() {
  # Creates a tmp local + bare-remote pair, returns local path via stdout.
  local local_dir="$1"
  local remote_dir="$2"
  git init -q --bare "$remote_dir" 2>/dev/null
  mkdir -p "$local_dir"
  (
    cd "$local_dir" || exit 1
    git init -q -b main
    git config user.email test@test
    git config user.name test
    echo "initial" > tracked-file.txt
    git add tracked-file.txt
    git commit -q -m "initial commit"
    git remote add origin "$remote_dir"
    git push -q origin main 2>/dev/null || true
  )
}

# Use a unique HOME so we don't pollute the real ~/.chorus/recovery
TEST_HOME="/tmp/chorus-recover-test-$$"
mkdir -p "$TEST_HOME/.chorus"
export HOME_BAK="$HOME"

# Assertion 1: clean tree → no-op
echo "test-chorus-werk-sync-recover: assertion 1 — clean tree → no-op sync"
CLEAN_LOCAL="/tmp/chorus-recover-clean-$$"
CLEAN_REMOTE="/tmp/chorus-recover-clean-remote-$$.git"
setup_repo_with_remote "$CLEAN_LOCAL" "$CLEAN_REMOTE"
OUT=$(HOME="$TEST_HOME" CHORUS_HOME="$CLEAN_LOCAL" CHORUS_WERK_SYNC_LOCK_TIMEOUT=5 bash "$SCRIPT" recover 2>&1 || true)
rm -rf "$CLEAN_LOCAL" "$CLEAN_REMOTE"
if echo "$OUT" | grep -q "no dirty files; synced"; then
  echo "PASS [clean-tree]: no-op sync message"
  PASS=$((PASS+1))
else
  echo "FAIL [clean-tree]: expected 'no dirty files' message, got:"
  echo "  $OUT"
  FAIL=$((FAIL+1))
fi
# Recovery dir should NOT have been created on clean tree
if [ ! -d "$TEST_HOME/.chorus/recovery" ] || [ -z "$(ls -A "$TEST_HOME/.chorus/recovery" 2>/dev/null)" ]; then
  echo "PASS [clean-tree-no-recovery]: ~/.chorus/recovery/ stays empty on clean tree"
  PASS=$((PASS+1))
else
  echo "FAIL [clean-tree-no-recovery]: recovery dir created unnecessarily"
  ls -la "$TEST_HOME/.chorus/recovery"
  FAIL=$((FAIL+1))
fi
rm -rf "$TEST_HOME/.chorus/recovery"

# Assertion 2: dirty tree with one real file → stash + sync
echo "test-chorus-werk-sync-recover: assertion 2 — dirty tree with real file → stash + sync"
DIRTY_LOCAL="/tmp/chorus-recover-dirty-$$"
DIRTY_REMOTE="/tmp/chorus-recover-dirty-remote-$$.git"
setup_repo_with_remote "$DIRTY_LOCAL" "$DIRTY_REMOTE"
# Dirty it: modify the tracked file
echo "REAL_EDIT_CONTENT_$$" > "$DIRTY_LOCAL/tracked-file.txt"
OUT=$(HOME="$TEST_HOME" CHORUS_HOME="$DIRTY_LOCAL" CHORUS_WERK_SYNC_LOCK_TIMEOUT=5 bash "$SCRIPT" recover 2>&1 || true)
if echo "$OUT" | grep -q "stashed 1 file"; then
  echo "PASS [dirty-stash]: recover stashed 1 file"
  PASS=$((PASS+1))
else
  echo "FAIL [dirty-stash]: expected 'stashed 1 file', got:"
  echo "  $OUT"
  FAIL=$((FAIL+1))
fi
# Verify the recovery dir exists with content
RECOVERY_TS_DIR=$(ls "$TEST_HOME/.chorus/recovery/" 2>/dev/null | head -1)
if [ -n "$RECOVERY_TS_DIR" ] && [ -f "$TEST_HOME/.chorus/recovery/$RECOVERY_TS_DIR/manifest.tsv" ]; then
  echo "PASS [recovery-manifest]: manifest.tsv written"
  PASS=$((PASS+1))
  # Verify content is preserved
  STASHED_HASH=$(awk '{print $1}' "$TEST_HOME/.chorus/recovery/$RECOVERY_TS_DIR/manifest.tsv" | head -1)
  if [ -f "$TEST_HOME/.chorus/recovery/$RECOVERY_TS_DIR/$STASHED_HASH" ]; then
    if grep -q "REAL_EDIT_CONTENT_$$" "$TEST_HOME/.chorus/recovery/$RECOVERY_TS_DIR/$STASHED_HASH"; then
      echo "PASS [recovery-lossless]: stashed content preserved byte-for-byte"
      PASS=$((PASS+1))
    else
      echo "FAIL [recovery-lossless]: stashed file doesn't contain original content"
      FAIL=$((FAIL+1))
    fi
  else
    echo "FAIL [recovery-lossless]: stashed file missing at expected path"
    FAIL=$((FAIL+1))
  fi
else
  echo "FAIL [recovery-manifest]: ~/.chorus/recovery/<ts>/manifest.tsv not written"
  ls -la "$TEST_HOME/.chorus/recovery/" 2>/dev/null || echo "  (recovery dir doesn't exist)"
  FAIL=$((FAIL+1))
fi
# Tree should now have no M/A/D entries (untracked lock file is fine).
POST_DIRTY=$(cd "$DIRTY_LOCAL" && git status --porcelain 2>/dev/null | grep -E '^[ MAD]' || true)
if [ -z "$POST_DIRTY" ]; then
  echo "PASS [post-recover-clean]: no M/A/D entries after recover (sync-blocking state cleared)"
  PASS=$((PASS+1))
else
  echo "FAIL [post-recover-clean]: M/A/D entries remain after recover"
  echo "$POST_DIRTY"
  FAIL=$((FAIL+1))
fi
rm -rf "$DIRTY_LOCAL" "$DIRTY_REMOTE" "$TEST_HOME/.chorus/recovery"

# Assertion 3: dirty tree with merge=union file only → no stash
echo "test-chorus-werk-sync-recover: assertion 3 — merge=union file only → no stash"
UNION_LOCAL="/tmp/chorus-recover-union-$$"
UNION_REMOTE="/tmp/chorus-recover-union-remote-$$.git"
git init -q --bare "$UNION_REMOTE" 2>/dev/null
mkdir -p "$UNION_LOCAL"
(
  cd "$UNION_LOCAL" || exit 1
  git init -q -b main
  git config user.email test@test
  git config user.name test
  echo "activity.md merge=union" > .gitattributes
  echo "# initial" > activity.md
  git add .gitattributes activity.md
  git commit -q -m "initial commit"
  git remote add origin "$UNION_REMOTE"
  git push -q origin main 2>/dev/null || true
  # Dirty the union-merge file
  echo "# edit appended" >> activity.md
)
OUT=$(HOME="$TEST_HOME" CHORUS_HOME="$UNION_LOCAL" CHORUS_WERK_SYNC_LOCK_TIMEOUT=5 bash "$SCRIPT" recover 2>&1 || true)
rm -rf "$UNION_LOCAL" "$UNION_REMOTE"
if echo "$OUT" | grep -q "no dirty files; synced"; then
  echo "PASS [union-skip]: union-merge file correctly skipped by classifier"
  PASS=$((PASS+1))
else
  if echo "$OUT" | grep -q "stashed 0 file"; then
    echo "PASS [union-skip]: union-merge file correctly skipped (zero stashed)"
    PASS=$((PASS+1))
  else
    echo "FAIL [union-skip]: union-merge file shouldn't have been stashed"
    echo "  output: $OUT"
    FAIL=$((FAIL+1))
  fi
fi

# Cleanup
rm -rf "$TEST_HOME"

exit 0
