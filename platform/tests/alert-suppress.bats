#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# Tests for #2305: Suppress alerts during planned restarts
# What Jeff sees: planned deploys/restarts fire alerts that look like real outages.
# After this: app-state.sh writes a suppress file, deep-health.sh respects it.

DEEP_HEALTH="${CHORUS_ROOT}/platform/scripts/deep-health.sh"
APP_STATE="${CHORUS_ROOT}/platform/scripts/app-state.sh"
SUPPRESS_FILE="/tmp/chorus-alert-suppress"

setup() {
  # Clean suppress file before each test
  rm -f "$SUPPRESS_FILE"
}

teardown() {
  rm -f "$SUPPRESS_FILE"
}

# --- AC 1: app-state.sh writes suppress file on deploy/restart ---

@test "app-state.sh restart writes suppress file with TTL" {
  # Source just the suppress function (we'll test the function, not the full command)
  # The suppress file should contain an expiry epoch
  bash "$APP_STATE" suppress 60
  [ -f "$SUPPRESS_FILE" ]
  expiry=$(cat "$SUPPRESS_FILE")
  now=$(date +%s)
  # Expiry should be in the future (now + ~60s)
  [ "$expiry" -gt "$now" ]
}

# --- AC 2: deep-health.sh skips alerting during suppress window ---

@test "deep-health skips alerting when suppress file is active" {
  # Write a suppress file that expires 120s from now
  echo $(( $(date +%s) + 120 )) > "$SUPPRESS_FILE"
  run bash "$DEEP_HEALTH"
  # Should exit 0 and report suppressed, not fire alerts
  [ "$status" -eq 0 ]
  [[ "$output" == *"suppressed"* ]]
}

# --- AC 3: suppress file auto-expires ---

@test "deep-health ignores expired suppress file" {
  # Write a suppress file that already expired
  echo $(( $(date +%s) - 60 )) > "$SUPPRESS_FILE"
  run bash "$DEEP_HEALTH"
  # Should run normally — expired suppress file treated as absent
  # (may pass or fail depending on actual health, but should NOT say "suppressed")
  [[ "$output" != *"suppressed"* ]]
}

# --- AC 4: manual override ---

@test "app-state.sh suppress command accepts custom seconds" {
  bash "$APP_STATE" suppress 300
  [ -f "$SUPPRESS_FILE" ]
  expiry=$(cat "$SUPPRESS_FILE")
  now=$(date +%s)
  diff=$(( expiry - now ))
  # Should be roughly 300s in the future (allow 5s tolerance)
  [ "$diff" -ge 295 ]
  [ "$diff" -le 305 ]
}

# --- AC 5: alerts after window expires are real ---

@test "alerts fire normally with no suppress file" {
  rm -f "$SUPPRESS_FILE"
  run bash "$DEEP_HEALTH"
  # Normal behavior — passes or fails based on actual health
  # Key: no "suppressed" in output
  [[ "$output" != *"suppressed"* ]]
}
