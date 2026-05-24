#!/usr/bin/env bash
# test-crawler-busy-timeout.sh — #3073: the crawler's sqlite writes must
# survive a briefly-locked index DB (busy_timeout waits+retries) instead of
# dying with exit=5 (SQLITE_BUSY), and a genuine lock must be VISIBLE (no
# suppressed stderr) rather than a silent death.
#
# RCA (2026-05-24): index-crawler-snapshots.sh had unguarded sqlite3
# DELETE/INSERT writes with 2>/dev/null and no busy_timeout. Under
# `set -euo pipefail` a locked DB → SQLITE_BUSY (code 5) → silent exit 5.
#
# Behavioral tests (1-3) prove the busy_timeout pattern is correct.
# Structural guards (4-5) prove the actual script adopts it.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SCRIPT="$CHORUS_ROOT/platform/scripts/index-crawler-snapshots.sh"

TEST_DB=$(mktemp -t crawler-busy.XXXXXX.db)
cleanup() { rm -f "$TEST_DB" "$TEST_DB"-wal "$TEST_DB"-shm /tmp/busy-t1.err /tmp/busy-t2.err /tmp/busy-t3.err 2>/dev/null; }
trap cleanup EXIT

# Minimal messages table mirroring the columns the crawler writes.
sqlite3 "$TEST_DB" "CREATE TABLE messages (source TEXT, source_id TEXT, channel TEXT, role TEXT, author TEXT, content TEXT, timestamp TEXT, metadata TEXT);"

# Hold a write lock (RESERVED via BEGIN IMMEDIATE) for $1 seconds, in background.
hold_write_lock() {
  local secs="$1"
  # stdout/stderr redirected so $() command-substitution returns immediately
  # (otherwise it blocks until the backgrounded locker closes the pipe = lock released).
  python3 -c "
import sqlite3, time
con = sqlite3.connect('$TEST_DB', timeout=30)
con.isolation_level = None
con.execute('BEGIN IMMEDIATE')   # acquire RESERVED lock now
time.sleep($secs)
con.execute('COMMIT')
con.close()
" >/dev/null 2>&1 &
  echo $!
}

INSERT_SQL="INSERT INTO messages (source, source_id, channel, role, author, content, timestamp, metadata) VALUES ('crawler','x','crawl:test','system','crawler','snap','2026-01-01','{}');"

echo "=== #3073: crawler sqlite write resilience (busy_timeout) ==="

# --- Test 1: WITH busy_timeout, a brief lock is survived (waits + succeeds) ---
LOCKER=$(hold_write_lock 1)
sleep 0.3   # ensure BEGIN IMMEDIATE has executed
if sqlite3 "$TEST_DB" "PRAGMA busy_timeout=5000; $INSERT_SQL" 2>/tmp/busy-t1.err; then
  p "busy_timeout=5000 survived a ~1s lock (waited + wrote)"
else
  f "busy_timeout write should have survived a brief lock ($(cat /tmp/busy-t1.err))"
fi
wait "$LOCKER" 2>/dev/null || true

# --- Test 2 (red baseline): NO busy_timeout fails fast against a held lock ---
LOCKER=$(hold_write_lock 1)
sleep 0.3
sqlite3 "$TEST_DB" "$INSERT_SQL" 2>/tmp/busy-t2.err
RC=$?
if [ "$RC" -eq 5 ]; then
  p "no-busy_timeout write fails fast with code 5 (SQLITE_BUSY) — proves the fix is load-bearing"
elif [ "$RC" -ne 0 ]; then
  p "no-busy_timeout write fails (code $RC) against a held lock — fix is load-bearing"
else
  f "expected no-busy_timeout write to FAIL against a held lock, but it succeeded (RC=0)"
fi
wait "$LOCKER" 2>/dev/null || true

# --- Test 3: a sustained lock beyond the timeout is VISIBLE, not silent ---
LOCKER=$(hold_write_lock 3)
sleep 0.3
sqlite3 "$TEST_DB" "PRAGMA busy_timeout=300; $INSERT_SQL" 2>/tmp/busy-t3.err
RC=$?
if [ "$RC" -ne 0 ] && grep -qiE "lock|busy" /tmp/busy-t3.err; then
  p "sustained lock beyond busy_timeout surfaces a visible 'locked' error (code $RC, stderr captured)"
else
  f "expected visible lock error on sustained lock, got RC=$RC stderr='$(cat /tmp/busy-t3.err)'"
fi
wait "$LOCKER" 2>/dev/null || true

# --- Test 4 (structural): the script's writes carry a busy timeout ---
# Accept either form: `PRAGMA busy_timeout=N` or the CLI dot-command `.timeout N`.
BT_COUNT=$(grep -cE "busy_timeout|\.timeout " "$SCRIPT" 2>/dev/null | tr -d '[:space:]')
if [ "${BT_COUNT:-0}" -ge 2 ]; then
  p "script applies a busy timeout to its writes ($BT_COUNT occurrences)"
else
  f "expected a busy timeout on both DELETE+INSERT writes, found ${BT_COUNT:-0}"
fi

# --- Test 5 (structural): the crawler sqlite writes no longer suppress stderr ---
SUPPRESSED=$(grep -nE 'sqlite3 "\$DB_PATH".*2>/dev/null' "$SCRIPT" 2>/dev/null | grep -viE "SELECT 1" | wc -l | tr -d '[:space:]')
if [ "${SUPPRESSED:-0}" -eq 0 ]; then
  p "crawler sqlite writes no longer suppress stderr (a real lock will be visible)"
else
  f "found $SUPPRESSED crawler sqlite write(s) still suppressing stderr with 2>/dev/null"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
