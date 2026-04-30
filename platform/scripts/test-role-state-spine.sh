#!/bin/bash
# test-role-state-spine.sh — Tests for role-state spine event emission (#1945)
# AC: role-state emits role.state.changed to chorus.log on every transition

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0
CHORUS_LOG="${CHORUS_ROOT}/platform/logs/chorus.log"
ROLE_STATE="${CHORUS_ROOT}/platform/scripts/role-state"

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

# --- Test 3: retired by #2467/#2629 ---
# Original (89279b82, #1945) asserted role.state.changed events contained
# card=N. Wave 1 of #2467 (PR #72) dropped card from JSON output; wave 3
# (#2629) refuses card= at the CLI. Behavior is deliberately gone —
# board is source of truth for cards. Retired per eliminate-vs-manage,
# not @skip-tagged.

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
