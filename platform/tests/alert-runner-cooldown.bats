#!/usr/bin/env bats
# alert-runner-cooldown.bats — Tests for #1861
# What Jeff sees: 3 alerts firing every 5-10 min despite cooldown files existing

RUNNER="/Users/jeffbridwell/CascadeProjects/chorus/proving/scripts/alert-runner.sh"

@test "alert-runner exists and is executable" {
  [ -x "$RUNNER" ]
}

@test "alert-runner delegates all nudge logic to action block" {
  # #1985: runner no longer checks cooldown files or nudges directly.
  # Action block in the YAML owns consecutive-failure tracking, cooldown, and nudge.
  # Runner just logs FIRE and runs the action block.
  grep -q "ACTION.*fired\|action_script" "$RUNNER"
}

@test "alert-runner does not have independent nudge cooldown" {
  # The old 10-min nudge cooldown (alert-nudge-*) should be removed
  # in favor of respecting the action block's cooldown
  ! grep -q "alert-nudge-" "$RUNNER"
}

@test "alert-runner does not nudge independently of action block" {
  # #1985: runner had its own nudge path that bypassed the action block's
  # consecutive-failure counter. The runner should NOT call nudge directly.
  # Only the action block (inside the YAML) should nudge.
  # Count nudge calls outside of action block execution
  local runner_nudge_calls=$(grep -c 'scripts/nudge' "$RUNNER")
  [ "$runner_nudge_calls" -eq 0 ]
}
