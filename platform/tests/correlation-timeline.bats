#!/usr/bin/env bats
# @test-type: unit — retirement guard
load test_helper
# correlation-timeline.bats — RETIREMENT GUARD (#3710)
#
# #2280 shipped platform/scripts/correlation-timeline.sh ("what happened around
# this alert?" — a merged timeline across spine/hooks/git/alerts). #2035's dead
# code sweep (Wren, accepted) deleted it as an orphan: nothing called it.
#
# The deletion was right; leaving these tests behind was not. Seven cases went
# on asserting `[ -x "$SCRIPT" ]` against a file that no longer existed, so this
# file has been red on every run since — noise that trained everyone to ignore
# the suite, which is exactly what #3710 is cleaning up.
#
# Per the retirement-gate convention: when a surface is retired, the test
# asserts its ABSENCE. That keeps the removal deliberate — if the script ever
# comes back it should come back through a card that also restores real
# coverage, not silently reappear under tests that only checked it existed.

RETIRED_SCRIPT="${CHORUS_ROOT}/platform/scripts/correlation-timeline.sh"

@test "correlation-timeline.sh stays retired (#2035 dead-code sweep)" {
  [ ! -e "$RETIRED_SCRIPT" ]
}

@test "nothing in the tree calls correlation-timeline.sh" {
  # A caller appearing without the script is the failure mode worth catching:
  # it would mean someone wired up a command that cannot run.
  run bash -c "grep -rl 'correlation-timeline' '${CHORUS_ROOT}/platform/scripts' '${CHORUS_ROOT}/.github' 2>/dev/null || true"
  [ -z "$output" ]
}
