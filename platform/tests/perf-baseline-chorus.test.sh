#!/usr/bin/env bash
# perf-baseline-chorus.test.sh — Verify Chorus perf baseline script
# Card #1914 AC: covers 4 endpoints, thresholds, appends to nightly log
# Run: bash platform/tests/perf-baseline-chorus.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/perf-baseline-chorus.sh"

echo "=== Perf Baseline Chorus Tests (#1914) ==="

# Test 1: Script exists and is executable
if [ -x "$SCRIPT" ]; then
  test_pass "Script is executable"
else
  test_fail "Script missing or not executable"
fi

# Test 2: Script has valid syntax
if bash -n "$SCRIPT" 2>/dev/null; then
  test_pass "Script has valid bash syntax"
else
  test_fail "Script has syntax errors"
fi

# Test 3: Script runs and exits 0
OUTPUT=$(bash "$SCRIPT" 2>&1)
EXIT=$?
if [ "$EXIT" -eq 0 ]; then
  test_pass "Script exits 0"
else
  test_fail "Script exited $EXIT"
fi

# Test 4-7: All 4 endpoints appear in output
for endpoint in "chorus-api" "clearing" "fuseki" "vikunja"; do
  if echo "$OUTPUT" | grep -qi "$endpoint"; then
    test_pass "$endpoint appears in output"
  else
    test_fail "$endpoint missing from output"
  fi
done

# Test 8: Each result line has a response time in ms
ms_lines=$(echo "$OUTPUT" | grep -c 'ms' || true)
if [ "$ms_lines" -ge 4 ]; then
  test_pass "At least 4 endpoints have ms response times"
else
  test_fail "Only $ms_lines endpoints have ms response times (need 4)"
fi

# Test 9: Each result has pass/fail threshold judgment
pf_lines=$(echo "$OUTPUT" | grep -cE 'pass|FAIL' || true)
if [ "$pf_lines" -ge 4 ]; then
  test_pass "At least 4 endpoints have pass/fail thresholds"
else
  test_fail "Only $pf_lines endpoints have pass/fail judgment (need 4)"
fi

# Test 10: Results appended to perf-baseline-nightly.log
LOG="$HOME/Library/Logs/Chorus/perf-baseline-nightly.log"
if [ -f "$LOG" ] && tail -10 "$LOG" | grep -q "chorus-api"; then
  test_pass "Results appended to perf-baseline-nightly.log"
else
  test_fail "Results not found in perf-baseline-nightly.log"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
