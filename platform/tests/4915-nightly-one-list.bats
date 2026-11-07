#!/usr/bin/env bats
# @test-type: unit — werk-test's notify_messages over planted suite rows (cargo test)
# @domain: tests
# @card: #4915 · owner: kade
# AC5: one nightly red list covers every role's suites. Each owner's own line carries
# only its own, so no message showed the whole night; the TOTAL now names every red
# suite by owner and reaches Wren (who gives Jeff the line) as well as Kade.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "the TOTAL names every red suite of every owner and reaches wren (with its negative proof)" {
  run cargo test -q --manifest-path "$ROOT/platform/services/werk-test/Cargo.toml" --lib \
    total_names_every_red_suite_of_every_owner_and_reaches_wren
  echo "$output" | tail -5
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run (renamed or filtered away)"; return 1; }
}
