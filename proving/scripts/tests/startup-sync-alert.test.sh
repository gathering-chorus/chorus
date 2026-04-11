#!/usr/bin/env bash
# startup-sync-alert.test.sh — Verify startup-sync alert checks Fuseki health before firing
# Card #1895

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

ALERT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/proving/domains/alerts/startup-sync-failure.yml"

echo "=== Startup Sync Alert Tests (#1895) ==="

# Test 1: alert rule exists
if [[ -f "$ALERT" ]]; then
  test_pass "startup-sync-failure.yml exists"
else
  test_fail "startup-sync-failure.yml not found"
fi

# Test 2: alert checks Fuseki health before firing
if grep -q 'localhost:3030\|fuseki.*health\|fuseki.*ready' "$ALERT"; then
  test_pass "alert checks Fuseki health before firing"
else
  test_fail "alert does not check Fuseki health — will fire cascade noise after restart"
fi

# Test 3: alert suppresses when Fuseki is healthy
if grep -q 'fuseki.*ok\|FUSEKI_OK\|fuseki.*200' "$ALERT"; then
  test_pass "alert has suppression path when Fuseki is healthy"
else
  test_fail "alert missing suppression path for healthy Fuseki"
fi

# Test 4: alert still fires when Fuseki is genuinely down
if grep -q 'failed\|exit 1' "$ALERT"; then
  test_pass "alert still fires on genuine failures"
else
  test_fail "alert lost failure detection"
fi

echo ""
echo "Results: $PASS pass, $FAIL fail"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
