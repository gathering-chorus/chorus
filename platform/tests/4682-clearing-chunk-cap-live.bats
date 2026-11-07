#!/usr/bin/env bats
# @test-type: integration
# @domain: board
# 4682-clearing-chunk-cap-live.bats — live board test for clearing-chunk active chunk limit (#4682)

setup() {
  export SCRIPT="$BATS_TEST_DIRNAME/../scripts/clearing-chunk"
}

@test "clearing-chunk runs live against board and outputs active chunks count" {
  unset MOCK_BOARD_LISTING
  run "$SCRIPT" list
  [ "$status" -eq 0 ]
  [[ "$output" =~ "Active chunks" ]] || return 1
}
