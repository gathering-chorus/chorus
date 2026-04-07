#!/bin/bash
# test-clearing-ack.sh — Tests for Clearing Socket.IO ack (#1934)
# AC: jeff-message uses ack callback, client shows delivery state

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

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

# --- Test 2: Server calls callback with status ---
echo "Test 2: Server calls callback with delivery status"
assert_contains "callback called with status" "status.*sent\|status.*delivered" "$SERVER"

# --- Test 3: Client emit includes callback ---
echo "Test 3: Client emit includes ack callback"
# The emit should have a third argument (the callback function)
if grep -E "socket\.emit\('jeff-message'.*function\|socket\.emit\('jeff-message'.*=>" "$CLIENT" 2>/dev/null | grep -q .; then
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
