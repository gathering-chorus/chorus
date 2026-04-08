#!/usr/bin/env bash
# Test: deep-health.sh exists, runs, and detects failures (#2228)
# RED before script exists. GREEN after.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SCRIPT="$CHORUS_ROOT/platform/scripts/deep-health.sh"
PASS=0
FAIL=0

run_test() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== deep-health tests ==="

# 1. Script exists and is executable
run_test "script exists and is executable" test -x "$SCRIPT"

# 2. Script runs without crashing
run_test "script runs" bash "$SCRIPT"

# 3. Output contains either "all checks passed" or "failure(s)"
OUTPUT=$(bash "$SCRIPT" 2>/dev/null || true)
run_test "output is structured" echo "$OUTPUT" | grep -qE "(all checks passed|failure)"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
