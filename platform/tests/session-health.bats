#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# session-health.bats — Tests for session health monitoring (#2271)
# What Jeff sees: sessions degrade silently. These tests prove the system
# detects and signals when a session is getting long.

HEALTH_SCRIPT="${CHORUS_ROOT}/platform/scripts/session-health.sh"

# #4071 — every test here BRINGS its session (#3528). Until now tests 2-7 ran the
# script against Silas's LIVE transcript under ~/.claude/projects: green whenever
# a silas session happened to be open at 03:00, red whenever it was not — 12
# verdict flips across 43 runs with no change to the script. The fixture below
# is the only session any test sees; SESSION_HEALTH_SESSIONS_DIR is the seam the
# script already honours (test 9 used it since #3949).
setup() {
  FIX="$BATS_TEST_TMPDIR/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas"
  mkdir -p "$FIX"
  {
    for i in $(seq 1 12); do
      echo '{"type":"user","message":{"role":"user","content":"p'"$i"'"}}'
      echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash"}]}}'
    done
    echo '{"type":"queue-operation","operation":"remove"}'
  } > "$FIX/fixture-session.jsonl"
  export SESSION_HEALTH_SESSIONS_DIR="$BATS_TEST_TMPDIR/projects"
}
hs() { bash "$HEALTH_SCRIPT" "$@"; }

# --- AC 1: Session health metric emitted per prompt ---

@test "session-health script exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

@test "reports prompt count for active session" {
  run hs --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'prompts=[0-9]+'
}

@test "reports session age" {
  run hs --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'age_min=[0-9]+'
}

@test "reports tool call count" {
  run hs --role silas
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'tools=[0-9]+'
}

# --- AC 2: Alert threshold ---

@test "warns when session exceeds prompt threshold" {
  export SESSION_HEALTH_TEST=1
  run hs --role silas --threshold 1
  echo "$output" | grep -qiE 'warn|long|pressure'
}

# --- AC 3: Threshold research ---

@test "reports compaction rate as removes per 50 prompts" {
  run hs --role silas
  [ "$status" -eq 0 ]
  # Must emit a numeric remove_rate, not a placeholder string
  echo "$output" | grep -qE 'remove_rate=[0-9]+'
}

# --- AC 6: Compaction detection ---

@test "counts queue-operation remove events from session JSONL" {
  run hs --role silas
  [ "$status" -eq 0 ]
  # Must emit numeric queue_removes count, not 'not_emitted_by_claude_code'
  echo "$output" | grep -qE 'queue_removes=[0-9]+'
  # Must NOT contain the old placeholder
  ! echo "$output" | grep -q 'not_emitted_by_claude_code'
}

# --- Test-mode suppression ---

@test "does not fire nudges during test runs" {
  export SESSION_HEALTH_TEST=1
  run hs --role silas --threshold 1
  [ "$status" -eq 0 ]
  # Should still report WARN but nudge calls should be suppressed
  echo "$output" | grep -qiE 'warn|long|pressure'
}

# --- AC 2: Compaction rate alert ---

@test "warns when compaction rate exceeds threshold" {
  export SESSION_HEALTH_TEST=1
  # #3949 — the test BRINGS its session (#3528): a fixture transcript with
  # compaction-remove events, so the assert is about the script's logic, never
  # about whatever the live session happens to contain at 04:44.
  FIX="$BATS_TEST_TMPDIR/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-silas"
  mkdir -p "$FIX"
  {
    for i in $(seq 1 60); do
      echo '{"type":"user","message":{"role":"user","content":"p'"$i"'"}}'
    done
    echo '{"type":"queue-operation","operation":"remove"}'
    echo '{"type":"queue-operation","operation":"remove"}'
  } > "$FIX/fixture-session.jsonl"
  SESSION_HEALTH_SESSIONS_DIR="$BATS_TEST_TMPDIR/projects" \
    run hs --role silas --remove-rate-threshold 0
  echo "$output" | grep -qi 'compaction accelerating'
}

# #4071 NEGATIVE PROOF (#3734): with NO session in the world, the script must
# refuse loudly, not report numbers. This is the state the old tests hit at
# 03:00 whenever no silas session was open — and read as red for the wrong
# reason. Now that state is a typed refusal the fixture never enters.
@test "no session in the world -> exit 1 and no prompt count, never a number" {
  rm -rf "$BATS_TEST_TMPDIR/projects"
  mkdir -p "$BATS_TEST_TMPDIR/projects"
  run hs --role silas
  [ "$status" -ne 0 ] || { echo "reported on a session that does not exist: $output" >&2; return 1; }
  ! echo "$output" | grep -qE 'prompts=[0-9]+'
}
