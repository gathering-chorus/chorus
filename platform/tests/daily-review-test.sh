#!/usr/bin/env bash
# Test: daily review routing (#2243)
# AC 1: Summary posts ONE message to Bridge (not ops + quality separately)
# AC 2: Ops review includes alert state when alerts fired overnight
# AC 3: Quality aggregates all suites into one result line
# AC 4: infra-alert nudges role, does not post to Bridge
# AC 5: infra-alert suppresses after first fire per component per day
set -euo pipefail

SCRIPTS="/Users/jeffbridwell/CascadeProjects/platform/scripts"
INFRA_ALERT="/Users/jeffbridwell/CascadeProjects/platform/roles/silas/scripts/infra-alert.sh"
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

echo "=== daily-review routing tests (#2243) ==="

# --- AC 1: ops and quality do NOT post to Bridge independently ---
# When called from summary, ops/quality should only echo output, not bridge_post
run_test "ops script does not call bridge_post" \
  bash -c '! grep -q "bridge_post" "$0"' "$SCRIPTS/daily-review-ops.sh"

run_test "quality script does not call bridge_post" \
  bash -c '! grep -q "bridge_post" "$0"' "$SCRIPTS/daily-review-quality.sh"

run_test "summary script calls bridge_post exactly once" \
  bash -c '[ "$(grep -c "bridge_post" "$0")" -eq 1 ]' "$SCRIPTS/daily-review-summary.sh"

# --- AC 2: ops review checks alert state ---
run_test "ops script checks alert cooldown files" \
  bash -c 'grep -q "alert-nudge\|alert-state\|/tmp/alert" "$0"' "$SCRIPTS/daily-review-ops.sh"

# --- AC 3: quality aggregates into one line ---
# Quality should produce a single Tests: line, not multiple
run_test "quality output is single summary (no bridge_post, just echo)" \
  bash -c 'grep -q "echo.*BODY\|echo -e.*BODY" "$0" && ! grep -q "bridge_post" "$0"' "$SCRIPTS/daily-review-quality.sh"

# --- AC 4: infra-alert uses nudge, not Bridge ---
# Alert function must not POST to Bridge — health checks that GET from it are fine
run_test "infra-alert does not POST to Bridge" \
  bash -c '! grep -q "POST.*3470\|CLEARING_API.*message" "$0"' "$INFRA_ALERT"

run_test "infra-alert uses nudge for delivery" \
  bash -c 'grep -q "nudge" "$0"' "$INFRA_ALERT"

# --- AC 5: infra-alert has daily suppression ---
run_test "infra-alert checks daily fire state" \
  bash -c 'grep -q "\.fired\|daily.*suppress\|already.*fired" "$0"' "$INFRA_ALERT"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
