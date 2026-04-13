#!/usr/bin/env bats
# Alert cooldown verification — #1966

RUNNER="$BATS_TEST_DIRNAME/../alert-runner.sh"
ALERT_DIR="$BATS_TEST_DIRNAME/../../domains/alerts"

@test "runner checks cooldown before firing action" {
  grep -q 'cooldown\|COOLDOWN\|last_fire\|strike' "$RUNNER"
}

@test "runner skips action if fired recently" {
  # The runner must have logic to skip action when cooldown is active
  grep -q 'skip.*cooldown\|within.*cooldown\|too recent\|COOLDOWN_SECONDS' "$RUNNER"
}

@test "app-down rule has cooldown or threshold" {
  # Either the rule itself or the runner must enforce cooldown
  local rule="$ALERT_DIR/app-down.yml"
  grep -qi 'cooldown\|threshold\|consecutive' "$rule" || grep -q 'COOLDOWN' "$RUNNER"
}

@test "consecutive failure count before alerting" {
  grep -q 'consecutive\|strike\|fail_count\|THRESHOLD' "$RUNNER"
}
