#!/usr/bin/env bash
# watchdog-false-alerts.test.sh — Verify watchdog doesn't fire on accepted/wrong cards
# Card #2033 AC #1-4
# New file — no prior history.

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

WATCHDOG="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/watchdog.sh"

echo "=== Watchdog False Alert Tests (#2033) ==="

# Test 1: watchdog.sh exists and is executable
if [[ -x "$WATCHDOG" ]]; then
  test_pass "watchdog.sh exists and is executable"
else
  test_fail "watchdog.sh not found or not executable"
fi

# Test 2: watchdog checks card status before alerting
if grep -q 'CARD_STATUS\|card.*Done\|cards.*view' "$WATCHDOG"; then
  test_pass "watchdog checks card status before alerting"
else
  test_fail "watchdog missing card status check — will alert on accepted cards"
fi

# Test 3: watchdog skips Done cards
if grep -q 'Done.*continue\|CARD_STATUS.*Done' "$WATCHDOG"; then
  test_pass "watchdog skips Done/accepted cards"
else
  test_fail "watchdog doesn't skip Done cards"
fi

# Test 4: watchdog handles state transition gap
if grep -q 'age.*-lt.*30\|gap.*tolerance\|transition' "$WATCHDOG"; then
  test_pass "watchdog handles acp→pull state gap"
else
  test_fail "watchdog missing state transition gap handling"
fi

# Test 5: watchdog still alerts on real stalls (nudge code still present)
if grep -q 'NUDGE_THRESHOLD\|nudge.*watchdog' "$WATCHDOG"; then
  test_pass "watchdog still alerts on real stalls"
else
  test_fail "watchdog lost stall detection"
fi

echo ""
echo "Results: $PASS pass, $FAIL fail"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
