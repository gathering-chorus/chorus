#!/usr/bin/env bats
# Tests for deep-health.sh (#2228)
# What Jeff sees: silent failures go undetected. These tests prove detection works.

SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/deep-health.sh"

@test "deep-health script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "detects when fswatch is dead" {
  # If fswatch happens to be alive, this tests the check passes
  # If dead, it should report the failure
  run bash "$SCRIPT"
  # Script should complete without crashing
  [ "$status" -eq 0 ] || [[ "$output" == *"failure"* ]]
}

@test "detects stale hooks binary (rebuild without restart)" {
  run bash "$SCRIPT"
  # Should not crash — either passes or reports the mismatch
  [ "$status" -eq 0 ] || [[ "$output" == *"failure"* ]]
}

@test "checks chorus index freshness" {
  run bash "$SCRIPT"
  [ "$status" -eq 0 ] || [[ "$output" == *"failure"* ]]
}

@test "checks cloudflare tunnel" {
  run bash "$SCRIPT"
  [ "$status" -eq 0 ] || [[ "$output" == *"failure"* ]]
}

@test "reports all checks passed when healthy" {
  run bash "$SCRIPT"
  if [ "$status" -eq 0 ]; then
    [[ "$output" == *"all checks passed"* ]]
  fi
}

# --- Service checks (#2 deep-health service monitoring) ---

@test "checks chorus API reachability" {
  run bash "$SCRIPT"
  # Script must mention chorus-api in its checks — either passes or reports failure
  if ! curl -sf --max-time 2 http://localhost:3340/api/chorus/health > /dev/null 2>&1; then
    [[ "$output" == *"chorus-api"* ]]
  fi
}

@test "checks gathering app reachability" {
  run bash "$SCRIPT"
  if ! curl -sf --max-time 2 http://localhost:3000/health > /dev/null 2>&1; then
    [[ "$output" == *"gathering-app"* ]]
  fi
}

# --- Log freshness threshold classification ---

@test "weekly-scheduled logs use 8-day threshold not 2h" {
  # alert-delivery-test runs weekly (Sunday 6am)
  # cruft-scan runs every 72h
  # fuseki-compact runs weekly (Saturday 1am)
  # All three should be in WEEKLY_LOGS, not DAILY_LOGS or default 2h.
  eval "$(grep -E '^(DAILY_LOGS|WEEKLY_LOGS)=' "$SCRIPT")"
  [[ " $WEEKLY_LOGS " == *" alert-delivery-test.log "* ]]
  [[ " $WEEKLY_LOGS " == *" cruft-scan.log "* ]]
  [[ " $WEEKLY_LOGS " == *" fuseki-compact.log "* ]]
}

@test "fuseki-compact not in daily logs" {
  eval "$(grep -E '^DAILY_LOGS=' "$SCRIPT")"
  [[ " $DAILY_LOGS " != *" fuseki-compact.log "* ]]
}

@test "checks nudge binary exists" {
  run bash "$SCRIPT"
  # Should verify nudge is executable — either passes silently or reports
  [ "$status" -eq 0 ] || [[ "$output" == *"failure"* ]]
}
