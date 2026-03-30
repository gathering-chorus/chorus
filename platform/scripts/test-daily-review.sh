#!/bin/bash
# test-daily-review.sh — AC tests for #1807 daily review instrumentation
# Tests: spine events emitted, Bridge POST retry, ops.health.checked event, alert rule
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected '$expected', got '$actual')"
    ((FAIL++))
  fi
}

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -q "$needle"; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    ((FAIL++))
  fi
}

echo "=== #1807: Daily Review Instrumentation Tests ==="

# --- AC1: Each review step emits spine event on completion ---
echo ""
echo "Test 1: bridge-post helper exists and is sourceable"
if [ -f "$SCRIPT_DIR/lib/bridge-post.sh" ]; then
  source "$SCRIPT_DIR/lib/bridge-post.sh"
  echo "  PASS: lib/bridge-post.sh loaded"
  ((PASS++))
else
  echo "  FAIL: lib/bridge-post.sh not found"
  ((FAIL++))
fi

echo ""
echo "Test 2: bridge_post function available after sourcing"
if type bridge_post &>/dev/null; then
  echo "  PASS: bridge_post function defined"
  ((PASS++))
else
  echo "  FAIL: bridge_post function not defined"
  ((FAIL++))
fi

echo ""
echo "Test 3: ops review emits ops.review.completed spine event"
OPS_SCRIPT=$(cat "$SCRIPT_DIR/daily-review-ops.sh")
assert_contains "ops emits spine event" "chorus-log" "$OPS_SCRIPT"
assert_contains "ops emits ops.review.completed" "ops.review.completed" "$OPS_SCRIPT"

echo ""
echo "Test 4: quality review emits quality.review.completed spine event"
QUALITY_SCRIPT=$(cat "$SCRIPT_DIR/daily-review-quality.sh")
assert_contains "quality emits spine event" "chorus-log" "$QUALITY_SCRIPT"
assert_contains "quality emits quality.review.completed" "quality.review.completed" "$QUALITY_SCRIPT"

echo ""
echo "Test 5: summary review emits daily.review.completed spine event"
SUMMARY_SCRIPT=$(cat "$SCRIPT_DIR/daily-review-summary.sh")
assert_contains "summary emits spine event" "chorus-log" "$SUMMARY_SCRIPT"
assert_contains "summary emits daily.review.completed" "daily.review.completed" "$SUMMARY_SCRIPT"

# --- AC2: Bridge POST failures logged and retried once ---
echo ""
echo "Test 6: bridge_post retries on failure"
if type bridge_post &>/dev/null; then
  # Test with unreachable endpoint — should retry once, log both attempts
  RETRY_OUTPUT=$(bridge_post "http://localhost:19999/fake" "test" "test msg" 2>&1)
  RETRY_EXIT=$?
  assert_contains "logs retry attempt" "retry" "$RETRY_OUTPUT"
  assert_eq "returns non-zero on double failure" "1" "$RETRY_EXIT"
else
  echo "  FAIL: bridge_post not available for retry test"
  ((FAIL++))
  echo "  FAIL: (skipped exit code test)"
  ((FAIL++))
fi

echo ""
echo "Test 7: all three scripts use bridge_post instead of raw curl"
assert_contains "ops sources bridge-post" "bridge-post.sh" "$OPS_SCRIPT"
assert_contains "quality sources bridge-post" "bridge-post.sh" "$QUALITY_SCRIPT"
assert_contains "summary sources bridge-post" "bridge-post.sh" "$SUMMARY_SCRIPT"

# --- AC3: ops.health.checked event with pass/fail status ---
echo ""
echo "Test 8: ops review emits ops.health.checked with status"
assert_contains "ops.health.checked event in ops script" "ops.health.checked" "$OPS_SCRIPT"
assert_contains "status param in health event" "status=" "$OPS_SCRIPT"

# --- AC4: Alert rule — daily review didn't fire by 6:30am ---
echo ""
echo "Test 9: alert rule file exists with correct content"
ALERT_FILE="/Users/jeffbridwell/CascadeProjects/chorus/alerting/daily-review-missing.yml"
if [ -f "$ALERT_FILE" ]; then
  ALERT_CONTENT=$(cat "$ALERT_FILE")
  assert_contains "alert references daily.review.completed" "daily.review.completed" "$ALERT_CONTENT"
  assert_contains "alert has 6:30 threshold" "6:30" "$ALERT_CONTENT"
  echo "  PASS: alert file exists"
  ((PASS++))
else
  echo "  FAIL: alerting/daily-review-missing.yml not found"
  ((FAIL++))
  echo "  FAIL: (skipped content checks)"
  ((FAIL++))
  ((FAIL++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $((FAIL > 0 ? 1 : 0))
