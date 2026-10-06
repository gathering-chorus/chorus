#!/usr/bin/env bats
# @test-type: unit — fixture pin dirs via CWS_RUNS_DIR, no live state touched
# @domain: cicd
# @card: 4420 · owner: kade
# What Jeff sees: /cws says what a run really did — running, cancelled, or the
# tests that went red. werk-test runs bats suites and has no runner for a bare
# test-*.sh, so test-cws-3782.sh never ran in any pipeline until this wrapper
# (#4420 run 8: "gap: test-cws-3782.sh — no bats suite references this script").

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CWS="$REPO/platform/scripts/chorus-werk-status"

@test "the cws checks pass, negative proofs included" {
  [ -x "$CWS" ]
  run bash "$REPO/platform/tests/test-cws-3782.sh"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"test-cws-3782: all green"* ]] || return 1
}
