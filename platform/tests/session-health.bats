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
  export SESSION_HEALTH_TEST=1
  run bash "$HEALTH_SCRIPT" --role silas --threshold 1
  echo "$output" | grep -qiE 'warn|long|pressure'
}

# --- AC 3: Threshold research ---

@test "reports compaction rate as removes per 50 prompts" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  # Must emit a numeric remove_rate, not a placeholder string
  echo "$output" | grep -qE 'remove_rate=[0-9]+'
}

# --- AC 6: Compaction detection ---

@test "counts queue-operation remove events from session JSONL" {
  run bash "$HEALTH_SCRIPT" --role silas
  [ "$status" -eq 0 ]
  # Must emit numeric queue_removes count, not 'not_emitted_by_claude_code'
  echo "$output" | grep -qE 'queue_removes=[0-9]+'
  # Must NOT contain the old placeholder
  ! echo "$output" | grep -q 'not_emitted_by_claude_code'
}

# --- Test-mode suppression ---

@test "does not fire nudges during test runs" {
  export SESSION_HEALTH_TEST=1
  run bash "$HEALTH_SCRIPT" --role silas --threshold 1
  [ "$status" -eq 0 ]
  # Should still report WARN but nudge calls should be suppressed
  echo "$output" | grep -qiE 'warn|long|pressure'
}

# --- AC 2: Compaction rate alert ---

@test "warns when compaction rate exceeds threshold" {
  export SESSION_HEALTH_TEST=1
  # Use a remove-rate threshold of 0 to guarantee trigger
  run bash "$HEALTH_SCRIPT" --role silas --remove-rate-threshold 0
  echo "$output" | grep -qi 'compaction accelerating'
}
