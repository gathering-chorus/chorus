#!/usr/bin/env bats
# alert-runner-cooldown.bats — Tests for #1861
# What Jeff sees: 3 alerts firing every 5-10 min despite cooldown files existing

RUNNER="/Users/jeffbridwell/CascadeProjects/chorus/proving/scripts/alert-runner.sh"

@test "alert-runner exists and is executable" {
  [ -x "$RUNNER" ]
}

@test "alert-runner checks for action cooldown file before nudging" {
  # The runner should look for /tmp/alert-<name>-* cooldown files
  # from the action block, not just its own nudge cooldown
  grep -q "alert-cooldown\|COOLDOWN_FILE\|action.*cooldown" "$RUNNER"
}

@test "alert-runner does not have independent nudge cooldown" {
  # The old 10-min nudge cooldown (alert-nudge-*) should be removed
  # in favor of respecting the action block's cooldown
  ! grep -q "alert-nudge-" "$RUNNER"
}
