#!/usr/bin/env bats
# @test-type: smoke — runs role-post-all against the running principals; meaningful after a land and a start of each role
# @domain: identity
# @needs: machine — runs the real table against the running principals; a card run leaves it to the nightly (#4539)
#
# #4931 AC4 — done means the five-role table is all green live, not tests passing
# (Jeff 2026-11-08). This runs the real table; it is red until every agent principal
# has started once with the self test wired in, and red whenever any check fails.

@test "#4931 live: every agent principal green on every self-test check" {
  # #4925 — it was post-land only (CHORUS_LIVE_POST=1), so it skipped every night. #4931
  # landed long ago; as a machine test it now runs in the nightly, after the land, and a
  # principal that is not green is RED with its name (the answer #4931 AC4 wanted).
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run "$ROOT/platform/scripts/role-post-all"
  echo "$output"
  [ "$status" -eq 0 ]
}
