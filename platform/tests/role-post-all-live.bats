#!/usr/bin/env bats
# @test-type: smoke — runs role-post-all against the running principals; meaningful after a land and a start of each role
# @domain: identity
#
# #4931 AC4 — done means the five-role table is all green live, not tests passing
# (Jeff 2026-11-08). This runs the real table; it is red until every agent principal
# has started once with the self test wired in, and red whenever any check fails.

@test "#4931 live: every agent principal green on every self-test check" {
  # Reads the live roles wherever it runs (pipeline or prove); it never skips, so the
  # prove after the land sees a real measurement (a skip read as "unmeasured", 11:05).
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run "$ROOT/platform/scripts/role-post-all"
  echo "$output"
  [ "$status" -eq 0 ]
}
