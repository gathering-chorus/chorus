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

# Test 2: at least one alert YAML invokes ops-nudge (post-#2808;
# bash `nudge` was retired in #2804/#2809). Each alert's action block
# fires the nudge inline — checked across $ALERT_DIR.
if grep -lq 'ops-nudge' "$ALERT_DIR"/*.yml 2>/dev/null; then
  test_pass "alert YAMLs invoke ops-nudge"
else
  test_fail "no alert YAML invokes ops-nudge — alerts won't reach role terminals"
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

# Test 4: alert YAMLs use the canonical ops-nudge path (post-#2808)
if grep -lq 'platform/scripts/ops-nudge' "$ALERT_DIR"/*.yml 2>/dev/null; then
  test_pass "alert YAMLs use platform/scripts/ops-nudge path"
else
  test_fail "ops-nudge path not found in any alert YAML"
fi

echo ""
echo "Results: $PASS pass, $FAIL fail"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
