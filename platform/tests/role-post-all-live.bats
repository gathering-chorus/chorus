#!/usr/bin/env bats
# @test-type: smoke — runs role-post-all against the running principals; meaningful after a land and a start of each role
# @domain: identity
#
# #4931 AC4 — done means the five-role table is all green live, not tests passing
# (Jeff 2026-11-08). This runs the real table; it is red until every agent principal
# has started once with the self test wired in, and red whenever any check fails.

@test "#4931 live: every agent principal green on every self-test check" {
  # Before the land the running roles were started without the self test, so this can
  # only be measured after the land and one start of each role. It is run then, by hand
  # and by the post-land prove, with CHORUS_LIVE_POST=1; the output goes on the card.
  [ "${CHORUS_LIVE_POST:-0}" = 1 ] || skip "post-land only: run with CHORUS_LIVE_POST=1 after the land and a start of each role"
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run "$ROOT/platform/scripts/role-post-all"
  echo "$output"
  [ "$status" -eq 0 ]
}
