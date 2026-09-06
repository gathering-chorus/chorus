#!/usr/bin/env bats
# @test-type: unit — hermetic. Builds its own alert tree in BATS_TEST_TMPDIR and
# drives startup-sync-alert.test.sh against it. No live Fuseki, no network.
#
# #4111 — the alert test greps a rule that has not lived in one file since
# #3709 moved the check body into `startup-sync-failure.check.sh` beside the
# yml. It reported a WORKING alert as broken every night: 2 pass, 3 fail on the
# 2026-09-05 03:00 run, counted as PRODUCT BROKE. The rule lives across the
# pair, so the pair is what gets measured.
#
# The guard that matters is the negative one: a test that reads both files
# would also "pass" if it stopped reading either one, so each side is removed
# in turn and shown to put the assertions back to red.

setup() {
  SUT="$BATS_TEST_DIRNAME/../../proving/scripts/tests/startup-sync-alert.test.sh"
  SRC="$BATS_TEST_DIRNAME/../../proving/domains/alerts"
  TREE="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$TREE/proving/domains/alerts"
  cp "$SRC/startup-sync-failure.yml" "$TREE/proving/domains/alerts/"
  cp "$SRC/startup-sync-failure.check.sh" "$TREE/proving/domains/alerts/"
}

run_sut() { CHORUS_ROOT="$TREE" bash "$SUT"; }

@test "control: the real alert pair passes every assertion" {
  run run_sut
  [ "$status" -eq 0 ]
  [[ "$output" == *"5 pass, 0 fail"* ]]
}

@test "negative proof: empty the sidecar and the rule assertions go red" {
  # The exact state that produced last night's red — the yml alone.
  : > "$TREE/proving/domains/alerts/startup-sync-failure.check.sh"
  run run_sut
  [ "$status" -eq 1 ]
  [[ "$output" == *"2 pass, 3 fail"* ]]
  [[ "$output" == *"does not check Fuseki health"* ]]
}

@test "negative proof: delete the yml and the test still refuses, it does not pass on the sidecar alone" {
  # The mirror: reading the pair must not make either file optional.
  rm -f "$TREE/proving/domains/alerts/startup-sync-failure.yml"
  run run_sut
  [ "$status" -eq 1 ]
  [[ "$output" == *"not found"* ]]
}
