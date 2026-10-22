#!/usr/bin/env bats
# @test-type: unit
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

@test "clearing-chunk enforce refuses (exit 1) when board listing has no section headers (#4693 AC4 negative proof)" {
  export MOCK_BOARD_LISTING='unparseable error output without headers
  1 x [chunk:buzz]'
  run "$SCRIPT" enforce buzz
  [ "$status" -eq 1 ]
  [[ "$output" =~ "FATAL: failed to parse active chunks from board listing" ]] || return 1
}

@test "clearing-chunk enforce permits chunk when WIP is omitted from idle board with Next/Later headers (#4693 AC4)" {
  export MOCK_BOARD_LISTING='Next (2):
  4680 x [chunk:foo]
  4681 y [chunk:bar]
Later (1):
  4690 z [chunk:baz]'
  run "$SCRIPT" enforce buzz
  [ "$status" -eq 0 ]
  [[ "$output" =~ "activated chunk 'buzz' (1/3 active)" ]] || return 1
}

@test "clearing-chunk enforce permits chunk when WIP is explicit 0 (#4693 AC4)" {
  export MOCK_BOARD_LISTING='WIP (0):
Next (2):
  4680 x [chunk:foo]
  4681 y [chunk:bar]'
  run "$SCRIPT" enforce buzz
  [ "$status" -eq 0 ]
  [[ "$output" =~ "activated chunk 'buzz' (1/3 active)" ]] || return 1
}

@test "clearing-chunk check permits go when answered by Jeff" {
  run "$SCRIPT" check 4682 jeff go
  [ "$status" -eq 0 ]
  [[ "$output" =~ "plan approved by Jeff" ]] || return 1
}

@test "clearing-chunk check permits go when navigator is specified and differs from driver (#4693 AC5)" {
  run "$SCRIPT" check 4682 jeff go wren abby-normal
  [ "$status" -eq 0 ]
  [[ "$output" =~ "plan approved by Jeff" ]] || return 1
}

@test "clearing-chunk check refuses when navigator is empty (#4693 AC5 negative proof)" {
  run "$SCRIPT" check 4682 jeff go "" abby-normal
  [ "$status" -eq 1 ]
  [[ "$output" =~ "navigator cannot be empty" ]] || return 1
}

@test "clearing-chunk check refuses when navigator matches driver (#4693 AC5 negative proof)" {
  run "$SCRIPT" check 4682 jeff go abby-normal abby-normal
  [ "$status" -eq 1 ]
  [[ "$output" =~ "navigator 'abby-normal' cannot be the driver 'abby-normal'" ]] || return 1
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

@test "clearing-evaluate real dagu run skips evaluate step when card has no active chunk (#3734 negative proof)" {
  command -v dagu >/dev/null 2>&1 || skip "dagu not installed"
  WF="$BATS_TEST_DIRNAME/../pipelines/clearing-evaluate.yaml"
  export CHORUS_HOME="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  run dagu start "$WF" -- CARD=999999 ROLE=abby-normal NAVIGATOR=wren JEFF=jeff
  [ "$status" -eq 0 ]
  [[ "$output" =~ "skill-clearing-evaluate [skipped]" ]] || return 1
  [[ "$output" =~ "skill-clearing-loop [skipped]" ]] || return 1
  [[ "$output" =~ "skill-clearing-switch [skipped]" ]] || return 1
  [[ "$output" =~ "Result: Succeeded" ]] || return 1
}

@test "clearing-evaluate loop step has precondition gating on resequence decision" {
  WF="$BATS_TEST_DIRNAME/../pipelines/clearing-evaluate.yaml"
  grep -A 5 "skill-clearing-loop" "$WF" | grep -q 'expected: "resequence"' || return 1
}

@test "clearing-evaluate switch step has precondition gating on switch decision" {
  WF="$BATS_TEST_DIRNAME/../pipelines/clearing-evaluate.yaml"
  grep -A 5 "skill-clearing-switch" "$WF" | grep -q 'expected: "switch"' || return 1
}

@test "clearing-chunk switch fails (exit 1) if chorus-log fails (#3734 negative proof)" {
  _tmp=$(mktemp -d "${TMPDIR:-/tmp}/chunk-switch-neg.XXXXXX")
  echo '#!/bin/sh' > "$_tmp/bad-log"
  echo 'exit 1' >> "$_tmp/bad-log"
  chmod +x "$_tmp/bad-log"
  echo '#!/bin/sh' > "$_tmp/good-nudge"
  echo 'exit 0' >> "$_tmp/good-nudge"
  chmod +x "$_tmp/good-nudge"

  export CHORUS_LOG_SCRIPT="$_tmp/bad-log"
  export OPS_NUDGE_SCRIPT="$_tmp/good-nudge"

  run "$SCRIPT" switch 4693 ops abby-normal jeff
  rm -rf "$_tmp"
  [ "$status" -eq 1 ]
  [[ "$output" =~ "FATAL: failed to emit clearing.chunk.switched to spine" ]] || return 1
}

@test "clearing-chunk switch fails (exit 1) if ops-nudge fails (#3734 negative proof)" {
  _tmp=$(mktemp -d "${TMPDIR:-/tmp}/chunk-switch-neg.XXXXXX")
  echo '#!/bin/sh' > "$_tmp/good-log"
  echo 'exit 0' >> "$_tmp/good-log"
  chmod +x "$_tmp/good-log"
  echo '#!/bin/sh' > "$_tmp/bad-nudge"
  echo 'exit 1' >> "$_tmp/bad-nudge"
  chmod +x "$_tmp/bad-nudge"

  export CHORUS_LOG_SCRIPT="$_tmp/good-log"
  export OPS_NUDGE_SCRIPT="$_tmp/bad-nudge"

  run "$SCRIPT" switch 4693 ops abby-normal jeff
  rm -rf "$_tmp"
  [ "$status" -eq 1 ]
  [[ "$output" =~ "FATAL: failed to nudge jeff for next chunk" ]] || return 1
}

@test "clearing-chunk switch emits clearing.chunk.switched on spine (#4693 AC3)" {
  _tmp=$(mktemp -d "${TMPDIR:-/tmp}/chunk-switch.XXXXXX")
  export CHORUS_LOG_FILE="$_tmp/chorus.log"
  touch "$CHORUS_LOG_FILE"
  echo '#!/bin/sh' > "$_tmp/mock-nudge"
  echo 'exit 0' >> "$_tmp/mock-nudge"
  chmod +x "$_tmp/mock-nudge"
  export OPS_NUDGE_SCRIPT="$_tmp/mock-nudge"
  unset CHORUS_LOG_SCRIPT

  run "$SCRIPT" switch 4693 ops abby-normal jeff
  [ "$status" -eq 0 ]
  [[ "$output" =~ "lane switch from chunk 'ops'" ]] || return 1

  run python3 -c '
import sys, json
with open("'"$CHORUS_LOG_FILE"'") as f:
    events = [json.loads(line) for line in f if line.strip()]
sw = [e for e in events if e.get("event") == "clearing.chunk.switched"]
assert len(sw) == 1, f"expected 1 switch event, got {len(sw)}"
assert sw[0].get("chunk") == "ops"
assert sw[0].get("card_id") == 4693
assert "lane switch chosen by jeff" in sw[0].get("reason", "")
'
  rm -rf "$_tmp"
  [ "$status" -eq 0 ]
}
