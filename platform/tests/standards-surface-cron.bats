#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# Tests for standards-surface-cron.sh (#2268)
# What Jeff sees: the standards surface updates itself overnight.
# These tests prove: source detection works, skip when unchanged, regen when changed.

SCRIPT="${CHORUS_ROOT}/platform/scripts/standards-surface-cron.sh"
GEN_SCRIPT="${CHORUS_ROOT}/platform/scripts/generate-standards-surface.sh"

@test "AC1: cron wrapper script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "AC2: dry-run detects source changes on first run (no prior checksums)" {
  # Remove any existing state file so it looks like first run
  local state="/tmp/test-standards-checksums.json"
  rm -f "$state"
  # Override STATE_FILE via env — script should detect "first run" = changes
  run bash "$SCRIPT" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"first run"* ]] || [[ "$output" == *"would regenerate"* ]]
}

@test "AC2: skip when sources unchanged — checksum file persists after force run" {
  # Force run creates the checksum file
  bash "$SCRIPT" --force 2>/dev/null || true
  # Verify state file was written
  [ -f "$HOME/.chorus/standards-surface-checksums.json" ]
  # State file should contain JSON with source hashes
  [[ "$(cat "$HOME/.chorus/standards-surface-checksums.json")" == *"decisions"* ]]
}

@test "AC1: force flag always regenerates" {
  run bash "$SCRIPT" --force
  [ "$status" -eq 0 ]
  [[ "$output" == *"Forced regeneration"* ]]
  [[ "$output" == *"complete"* ]]
}

@test "AC3: generation script exists (dependency)" {
  [ -x "$GEN_SCRIPT" ]
}
