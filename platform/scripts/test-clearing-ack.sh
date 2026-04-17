#!/bin/bash
# test-clearing-ack.sh — Tests for Clearing Socket.IO ack (#1934)
# AC: jeff-message uses ack callback, client shows delivery state

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0
SERVER="${CHORUS_ROOT}/directing/clearing/src/server.ts"
CLIENT="${CHORUS_ROOT}/directing/clearing/public/index.html"

assert_contains() {
  local label="$1" needle="$2" file="$3"
  if grep -q "$needle" "$file" 2>/dev/null; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected '$needle' in $(basename "$file"))"
    ((FAIL++))
  fi
}

echo "=== Clearing Socket.IO ack tests ==="
echo ""

# --- Test 1: Server jeff-message handler has ack callback ---
echo "Test 1: Server handler accepts ack callback"
assert_contains "ack callback in handler signature" "callback\|ack" "$SERVER"

# --- Test 2: Server calls callback with delivery result ---
# Current shape (#1934): ack?.({ ok: boolean, error?: string }). Older fixture
# asserted "status: sent|delivered" — that API shape changed but the test
# wasn't updated.
echo "Test 2: Server calls callback with delivery result"
if grep -E "ack\??\.\(\{.*ok:" "$SERVER" 2>/dev/null | grep -q .; then
  echo "  PASS: callback invoked with { ok: ... }"
  ((PASS++))
else
  echo "  FAIL: callback called with { ok: ... } (expected in server.ts)"
  ((FAIL++))
fi

# --- Test 3: Client emit includes callback ---
# grep -E takes | as alternation directly; the old \| was literal backslash-pipe
# and never matched. Emit in client uses both function(result) and (ack) => styles.
echo "Test 3: Client emit includes ack callback"
if grep -E "socket\.emit\('jeff-message'.*(function|=>)" "$CLIENT" 2>/dev/null | grep -q .; then
  echo "  PASS: emit has callback"
  ((PASS++))
else
  echo "  FAIL: emit missing callback argument"
  ((FAIL++))
fi

# --- Test 4: Client shows delivery state ---
echo "Test 4: Send button shows delivery state"
assert_contains "sending state" "sending\|Sending" "$CLIENT"
assert_contains "sent state" "\.sent\|delivered" "$CLIENT"
assert_contains "failed state" "\.failed\|delivery-failed\|error" "$CLIENT"

# --- Test 5: Failed messages stay in input ---
echo "Test 5: Failed messages preserve input text"
assert_contains "input value restored on failure" "input.value\|restoreInput\|failedText" "$CLIENT"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
