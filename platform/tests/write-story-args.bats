#!/usr/bin/env bats
# @test-type: unit — hermetic. Reads write-story.sh's argument handling and
# nothing else: no Fuseki, no auth, no network, no cleanup.
#
# #4126 — split out of write-story.bats, which is declared needs-stack /
# integration for all six of its tests. Two of those six never needed a stack:
# "is the script executable" and "does it print usage with no args" are facts
# about a file on disk. Carrying them in the integration suite meant they only
# ran when the live graph store was up, and they paid that suite's cost — the
# whole file measured 649.4s on 2026-09-08, the second most expensive unit in
# the nightly.
#
# The four that genuinely write to Fuseki and read back stay where they are.
# This is not a rewrite of anyone's test; it is the two that were mislabelled,
# moved to where their own label says they belong.

SCRIPT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}/platform/scripts/write-story.sh"

@test "script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "missing args prints usage and exits non-zero" {
  run bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" =~ [Uu]sage ]]
}

# NEGATIVE PROOF (#3734) — this suite must be genuinely hermetic, not merely
# untested against a down stack. With every service unreachable it still has to
# pass; if a Fuseki call ever creeps back in, this goes red instead of quietly
# turning back into an integration test that happens to live in the fast lane.
@test "NEGATIVE PROOF: passes with no stack reachable at all" {
  run env FUSEKI_URL=http://127.0.0.1:1 CHORUS_API=http://127.0.0.1:1 \
      bash -c "[ -x '$SCRIPT' ] && bash '$SCRIPT' 2>&1 | grep -qi usage"
  [ "$status" -eq 0 ]
}
