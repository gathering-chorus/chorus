#!/usr/bin/env bats
# @test-type: smoke — runs role-post-all against the running principals; meaningful after a land and a start of each role
# @domain: identity
#
# #4931 AC4 — done means the five-role table is all green live, not tests passing
# (Jeff 2026-11-08). This runs the real table; it is red until every agent principal
# has started once with the self test wired in, and red whenever any check fails.

@test "#4931 live: every agent principal green on every self-test check" {
  # Reads the live roles wherever it runs (pipeline, nightly or the prove after a land).
  # Since #4931's first land every role writes its self-test result at each start, so this
  # passes whenever all agents are up and healthy, and goes red when one is not: a role
  # down or broken at 3am is exactly what the nightly should report. It never skips (a skip
  # read as "unmeasured" and failed the prove at 11:05).
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run "$ROOT/platform/scripts/role-post-all"
  echo "$output"
  [ "$status" -eq 0 ]
}
