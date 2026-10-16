#!/usr/bin/env bats
# @test-type: unit — a read-only fitness script against a fixture log, no live service
# @domain: tests — quality-health reads the test run log under its #4540 name
# What Jeff sees: platform/scripts/quality-health counts the last run's suites
# from WERK_TEST_LOG (renamed from NIGHTLY_LOG by #4540).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  QH="$ROOT/platform/scripts/quality-health"
  printf '=== Results: 3 passed, 1 failed ===\n' > "$BATS_TEST_TMPDIR/run.log"
}

@test "the suite counts come from WERK_TEST_LOG" {
  run env WERK_TEST_LOG="$BATS_TEST_TMPDIR/run.log" LOKI_URL=http://127.0.0.1:9 CHORUS_HOME="$ROOT" "$QH" --json
  [[ "$output" == *'"pass": 3'* ]] || return 1
  [[ "$output" == *'"fail": 1'* ]] || return 1
}

@test "negative proof: the old NIGHTLY_LOG name is no longer read" {
  run env NIGHTLY_LOG="$BATS_TEST_TMPDIR/run.log" WERK_TEST_LOG="$BATS_TEST_TMPDIR/missing.log" LOKI_URL=http://127.0.0.1:9 CHORUS_HOME="$ROOT" "$QH" --json
  [[ "$output" == *'"pass": 0'* ]] || return 1
  [[ "$output" != *'"pass": 3'* ]] || return 1
}
