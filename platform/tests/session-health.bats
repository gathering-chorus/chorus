#!/usr/bin/env bats
# session-health.bats — Tests for session health monitoring (#2271)
# What Jeff sees: sessions degrade silently. These tests prove the system
# detects and signals when a session is getting long.

HEALTH_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/session-health.sh"

# --- AC 1: Session health metric emitted per prompt ---

@test "session-health script exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

@test "reports prompt count for active session" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'prompts=[0-9]+'
}

@test "reports session age" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'age_min=[0-9]+'
}

@test "reports tool call count" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'tools=[0-9]+'
}

# --- AC 2: Alert threshold ---

@test "warns when session exceeds prompt threshold" {
  # Current session has 485+ prompts — should be above any reasonable threshold
  run bash "$HEALTH_SCRIPT" --role silas --threshold 100
  echo "$output" | grep -qiE 'warn|alert|reboot|long'
}

# --- AC 6: Compaction research ---

@test "reports whether compaction is detectable" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  # Output should include a compaction status line
  echo "$output" | grep -qiE 'compaction|queue_removes'
}
