#!/usr/bin/env bash
# alert-nudge.test.sh — Test that alert-runner nudges the owning role on fire
# Card #2037 AC #1, #2
# New file — no prior history.

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ALERT_DIR="$CHORUS_ROOT/proving/domains/alerts"
RUNNER="$CHORUS_ROOT/proving/scripts/alert-runner.sh"

echo "=== Alert Nudge Tests (#2037) ==="

# Test 1: alert-runner.sh exists and is executable
if [[ -x "$RUNNER" ]]; then
  test_pass "alert-runner.sh exists and is executable"
else
  test_fail "alert-runner.sh not found or not executable"
fi

# Test 2: alert-runner.sh contains nudge --force call
if grep -q 'nudge.*--force' "$RUNNER"; then
  test_pass "alert-runner.sh contains nudge --force"
else
  test_fail "alert-runner.sh missing nudge --force — alerts won't reach role terminals"
fi

# Test 3: all alert rules have action blocks that post to Bridge
for rule in "$ALERT_DIR"/*.yml; do
  name=$(grep '^name:' "$rule" | head -1 | sed 's/name: *//')
  if grep -q 'localhost:3470' "$rule"; then
    test_pass "$name posts to Bridge"
  else
    test_fail "$name missing Bridge POST"
  fi
done

# Test 4: nudge path uses the platform nudge script
if grep -q 'platform/scripts/nudge' "$RUNNER"; then
  test_pass "nudge uses platform/scripts/nudge path"
else
  test_fail "nudge path not found in alert-runner"
fi

echo ""
echo "Results: $PASS pass, $FAIL fail"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
