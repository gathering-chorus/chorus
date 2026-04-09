#!/usr/bin/env bash
# alert-runner-paths.test.sh — Verify alert-runner resolves paths to actual rule files
# Card #1837 AC #2: alerting pipeline fires end-to-end

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

RUNNER="/Users/jeffbridwell/CascadeProjects/chorus/proving/scripts/alert-runner.sh"
RULES_DIR="/Users/jeffbridwell/CascadeProjects/chorus/proving/domains/alerts"

echo "=== Alert Runner Path Tests (#1837) ==="

# Test 1: CHORUS_ROOT in alert-runner.sh resolves to chorus/ directory
chorus_root=$(grep '^CHORUS_ROOT=' "$RUNNER" | head -1 | sed 's/.*:-//' | sed 's/}"//')
if [[ "$chorus_root" == *"/chorus"* ]]; then
  test_pass "CHORUS_ROOT defaults to chorus/ directory"
else
  test_fail "CHORUS_ROOT defaults to '$chorus_root' — missing /chorus suffix"
fi

# Test 2: ALERT_DIR in alert-runner.sh points to a directory that exists
alert_dir_relative=$(grep '^ALERT_DIR=' "$RUNNER" | head -1 | sed 's/.*CHORUS_ROOT}//' | sed 's/"//')
resolved_dir=$(grep '^CHORUS_ROOT=' "$RUNNER" | head -1 | sed 's/.*:-//' | sed 's/}"//')
resolved_dir="${resolved_dir}${alert_dir_relative}"
if [[ -d "$resolved_dir" ]]; then
  test_pass "ALERT_DIR resolves to existing directory: $resolved_dir"
else
  test_fail "ALERT_DIR resolves to non-existent directory: $resolved_dir"
fi

# Test 3: Resolved ALERT_DIR contains .yml rule files
yml_count=$(ls "$resolved_dir"/*.yml 2>/dev/null | wc -l | tr -d ' ')
if [[ "$yml_count" -gt 0 ]]; then
  test_pass "ALERT_DIR contains $yml_count rule files"
else
  test_fail "ALERT_DIR contains zero .yml files — runner will process nothing"
fi

# Test 4: Dry-run alert-runner produces SKIP/OK/FIRE lines (not just start+complete)
output=$(CHORUS_ROOT=/Users/jeffbridwell/CascadeProjects/chorus bash "$RUNNER" --rule synthetic-test 2>&1 || true)
log_tail=$(tail -5 /Users/jeffbridwell/Library/Logs/Chorus/alert-runner.log 2>/dev/null)
if echo "$log_tail" | grep -qE '(OK |FIRE |SKIP )'; then
  test_pass "alert-runner processes rules (found SKIP/OK/FIRE in log)"
else
  test_fail "alert-runner produced no rule output — rules not being found"
fi

echo ""
echo "Results: $PASS pass, $FAIL fail"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
