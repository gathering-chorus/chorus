#!/bin/bash
# test-api-health.sh — Tests for /api/chorus/health endpoint (#2011)
# AC: Health endpoint returns db status, uptime, vector count, hook status

set -uo pipefail

PASS=0
FAIL=0
API="http://localhost:3340"

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qi "$needle"; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    ((FAIL++))
  fi
}

assert_status() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    ((PASS++))
  else
    echo "  FAIL: $label (expected HTTP $expected, got $actual)"
    ((FAIL++))
  fi
}

echo "=== /api/chorus/health tests ==="
echo ""

# --- Test 1: Endpoint exists and returns 200 ---
echo "Test 1: Health endpoint returns 200"
status=$(curl -s -o /dev/null -w '%{http_code}' "$API/api/chorus/health" 2>/dev/null)
assert_status "returns 200" "200" "$status"

# --- Test 2: Returns JSON ---
echo "Test 2: Response is JSON"
content_type=$(curl -s -I "$API/api/chorus/health" 2>/dev/null | grep -i content-type | head -1)
assert_contains "content-type is json" "application/json" "$content_type"

# --- Test 3: Has status field ---
echo "Test 3: Body contains status field"
body=$(curl -s "$API/api/chorus/health" 2>/dev/null)
assert_contains "has status" '"status"' "$body"

# --- Test 4: Has db field ---
echo "Test 4: Body contains db field"
assert_contains "has db" '"db"' "$body"

# --- Test 5: Has uptime field ---
echo "Test 5: Body contains uptime field"
assert_contains "has uptime" '"uptime"' "$body"

# --- Test 6: Has vectors field ---
echo "Test 6: Body contains vectors field"
assert_contains "has vectors" '"vectors"' "$body"

# --- Test 7: Vectors count > 0 ---
echo "Test 7: Vector count is positive"
vectors=$(echo "$body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('vectors',0))" 2>/dev/null)
if [ -n "$vectors" ] && [ "$vectors" -gt 0 ] 2>/dev/null; then
  echo "  PASS: vectors=$vectors"
  ((PASS++))
else
  echo "  FAIL: vectors should be > 0 (got '$vectors')"
  ((FAIL++))
fi

# --- Test 8: Has hooks field ---
echo "Test 8: Body contains hooks field"
assert_contains "has hooks" '"hooks"' "$body"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
