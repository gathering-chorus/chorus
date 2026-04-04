#!/bin/bash
# test-role-state-spine.sh — Tests for role-state spine event emission (#1945)
# AC: role-state emits role.state.changed to chorus.log on every transition

set -uo pipefail

PASS=0
FAIL=0
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log"
ROLE_STATE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/role-state"

echo "=== role-state spine event tests ==="
echo ""

# --- Test 1: State transition emits spine event ---
echo "Test 1: State transition emits role.state.changed"
BEFORE=$(wc -l < "$CHORUS_LOG" 2>/dev/null | tr -d ' ')
"$ROLE_STATE" silas waiting 2>/dev/null
sleep 1
AFTER=$(wc -l < "$CHORUS_LOG" 2>/dev/null | tr -d ' ')
NEW_LINES=$(tail -n +$((BEFORE + 1)) "$CHORUS_LOG" 2>/dev/null | grep "role.state.changed" | grep "silas")
if [ -n "$NEW_LINES" ]; then
  echo "  PASS: role.state.changed emitted for silas"
  ((PASS++))
else
  echo "  FAIL: no role.state.changed event after state transition"
  ((FAIL++))
fi

# --- Test 2: Event contains state value ---
echo "Test 2: Event contains the new state"
if echo "$NEW_LINES" | grep -q "waiting"; then
  echo "  PASS: event contains state=waiting"
  ((PASS++))
else
  echo "  FAIL: event missing state value"
  ((FAIL++))
fi

# --- Test 3: Building state with card emits card number ---
echo "Test 3: Building state emits card number"
BEFORE=$(wc -l < "$CHORUS_LOG" 2>/dev/null | tr -d ' ')
"$ROLE_STATE" silas building card=9999 2>/dev/null
sleep 1
NEW_LINES=$(tail -n +$((BEFORE + 1)) "$CHORUS_LOG" 2>/dev/null | grep "role.state.changed" | grep "silas")
if echo "$NEW_LINES" | grep -q "9999"; then
  echo "  PASS: event contains card=9999"
  ((PASS++))
else
  echo "  FAIL: event missing card number"
  ((FAIL++))
fi

# Reset state
"$ROLE_STATE" silas building card=2022 2>/dev/null

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
