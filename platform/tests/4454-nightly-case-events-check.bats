#!/usr/bin/env bats
# @test-type: unit — the comparison only; the live run is the script itself
# @domain: tests
# @card: 4454 · owner: kade
# Covers: platform/scripts/nightly-case-events-check
# What Jeff sees: the cases the /nightly page counts are the cases Loki can find
# by event. The 2026-10-09 03:00 run is the real negative proof: page 9,873
# passed, Loki 0 by `| json` (promtail kept only the message).

CHECK="${BATS_TEST_DIRNAME}/../scripts/nightly-case-events-check"

@test "self-test: a match passes; the stripped night, one missing case and an unreadable page do not" {
  run "$CHECK" --fixture
  [ "$status" -eq 0 ]
  [ "$output" = "NEGATIVE PROOF OK" ]
}

@test "an unreachable page is its own state, never a match" {
  CHORUS_NIGHTLY_URL=http://127.0.0.1:9/nightly run "$CHECK"
  [ "$status" -eq 2 ]
  [[ "$output" == "cannot read:"* ]] || return 1
}
