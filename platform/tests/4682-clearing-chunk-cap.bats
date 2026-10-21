#!/usr/bin/env bats
# @test-type: integration
# @domain: board
# 4682-clearing-chunk-cap.bats — tests for clearing-chunk active chunk limit enforcement (#4682)

setup() {
  export SCRIPT="$BATS_TEST_DIRNAME/../scripts/clearing-chunk"
}

@test "clearing-chunk enforce allows activating a chunk when under limit" {
  export MOCK_BOARD_LISTING='WIP (1):
  4682 Test card [Abby-normal|P1|chunk:pulse]
Next (2):
  4680 Another card [Wren|P2|chunk:buzz]'
  export TEAM_WIP_LIMIT=3

  run "$SCRIPT" enforce buzz
  [ "$status" -eq 0 ]
  [[ "$output" =~ "activated chunk 'buzz' (2/3 active)" ]] || return 1
}

@test "clearing-chunk enforce permits already active chunk even if limit reached" {
  export MOCK_BOARD_LISTING='WIP (2):
  4682 Test card [Abby-normal|P1|chunk:pulse]
  4683 Another card [Kade|P1|chunk:buzz]'
  export TEAM_WIP_LIMIT=2

  run "$SCRIPT" enforce pulse
  [ "$status" -eq 0 ]
  [[ "$output" =~ "chunk 'pulse' is already active (2/2 active)" ]] || return 1
}

@test "clearing-chunk enforce REFUSES activating new chunk when limit reached (#3734 negative proof)" {
  export MOCK_BOARD_LISTING='WIP (2):
  4682 Test card 1 [Abby-normal|P1|chunk:buzz]
  4683 Test card 2 [Kade|P1|chunk:pulse]'
  export TEAM_WIP_LIMIT=2

  run "$SCRIPT" enforce alerts-decisions
  [ "$status" -eq 1 ]
  [[ "$output" =~ "refused: active chunk limit reached (2): buzz, pulse — cannot activate 'alerts-decisions'" ]] || return 1
}

@test "clearing-chunk fails loudly if cards CLI fails or listing unparseable" {
  export MOCK_BOARD_LISTING=''
  run "$SCRIPT" enforce pulse
  [ "$status" -eq 1 ]
  [[ "$output" =~ "FATAL: empty board listing received" ]] || return 1
}

@test "clearing-chunk runs live against board and correctly detects pulse in WIP" {
  unset MOCK_BOARD_LISTING
  run "$SCRIPT" list
  [ "$status" -eq 0 ]
  [[ "$output" =~ "pulse" ]] || return 1
}

@test "clearing-chunk check permits go when answered by Jeff" {
  run "$SCRIPT" check 4682 jeff go
  [ "$status" -eq 0 ]
  [[ "$output" =~ "plan approved by Jeff" ]] || return 1
}

@test "clearing-chunk check refuses when answered by non-Jeff role (#3734 negative proof)" {
  run "$SCRIPT" check 4682 wren go
  [ "$status" -eq 1 ]
  [[ "$output" =~ "plan-go must come from Jeff, not 'wren'" ]] || return 1
}

@test "clearing-chunk check exits 2 with stopped when Jeff chooses stop" {
  run "$SCRIPT" check 4682 jeff stop
  [ "$status" -eq 2 ]
  [[ "$output" =~ "stopped by Jeff" ]] || return 1
}

@test "clearing-plan dry run skips pull step when plan-go is stopped (#3734 negative proof)" {
  command -v dagu >/dev/null 2>&1 || skip "dagu not installed"
  WF="$BATS_TEST_DIRNAME/../pipelines/clearing-plan.yaml"
  run dagu dry "$WF"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "skill-clearing-pull [skipped]" ]] || return 1
  [[ "$output" =~ "skill-clearing-pair [skipped]" ]] || return 1
  [[ "$output" =~ "Result: Succeeded" ]] || return 1
}
