#!/usr/bin/env bats
# @test-type: contract
# 4166 — athena-validate must RUN without anyone typing it, and its answer must
# reach a person.
#
# It was built on #3846 to sweep the live graph for the old/bad data the write
# door can never see. Jeff, 2026-09-13 13:09: "didnt we build athena-validate …
# isnt it meant to look for stuff in our graph that is causing problems". It is,
# and the first run in weeks found 1,204 issues — 1,151 dangling edges, 45
# untyped subjects, and 8 subjects living in more than one graph (pulse, spine,
# athena, chorus, borg, werk, loom, convergence). Nothing scheduled it.
#
# The failure this guards is not "the sweep is wrong". It is "the sweep is not
# running, and silence reads like health".

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/athena-validate.sh"
PLIST="$REPO_ROOT/platform/launchd/com.chorus.athena-validate.plist"

setup() { TMP="$(mktemp -d)"; }
teardown() { rm -rf "$TMP"; }

@test "a schedule exists in the repo, not only on one machine" {
  [ -f "$PLIST" ]
  grep -q "com.chorus.athena-validate" "$PLIST"
  grep -q "athena-validate.sh" "$PLIST"
  # It must actually be periodic — a plist with no cadence never fires.
  grep -qE "StartInterval|StartCalendarInterval" "$PLIST"
}

@test "the plist runs the canonical script and captures its output" {
  grep -q "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/athena-validate.sh" "$PLIST"
  grep -q "StandardOutPath" "$PLIST"
  grep -q "StandardErrorPath" "$PLIST"
}

@test "NEGATIVE PROOF — an unreachable store reports UNMEASURED, never 0 issues" {
  # Point it at a closed port. Silence and cleanliness must not look alike:
  # this is the failure that would let a dead sweep read as a healthy graph.
  run env FUSEKI_QUERY="http://127.0.0.1:9/query" bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" == *UNMEASURED* ]]
  [[ "$output" != *"PROVEN CLEAN"* ]]
}

@test "a run says what it found on the spine, with counts" {
  grep -q "graph.validate" "$SCRIPT"
}

@test "the one-home violations name the graphs a subject lives in, not just its name" {
  # "pulse is in 2 graphs" is not actionable; "pulse is in A and B" is.
  grep -q "GRAPH ?g" "$SCRIPT"
}
