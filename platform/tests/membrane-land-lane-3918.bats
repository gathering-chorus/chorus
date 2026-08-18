#!/usr/bin/env bats
# @test-type: integration
# #3918 — the land lane's own telemetry reaches the spine; a test's does not.
#
# This suite exists because the membrane could not separate the two states it
# is there to separate: act sets CI=true, the membrane called the werk pipeline
# a BUILD context, and every spine event the gate emitted panicked on the way
# out. The land happened; the record did not. Both directions are asserted here
# because fixing one by breaking the other is the failure mode (#3734).
load test_helper

CHORUS_LOG_BIN="${CHORUS_ROOT}/platform/scripts/chorus-log"

@test "land lane under act (CI set, CHORUS_CONTEXT=prod) reaches the spine" {
  run env CI=true CHORUS_CONTEXT=prod CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log" \
    BATS_TEST_TMPDIR= "$CHORUS_LOG_BIN" test.scoped silas card=3918 marker=lane-$$
  [ "$status" -eq 0 ]
  [[ "$output" != *"MEMBRANE REFUSED"* ]]
}

@test "NEGATIVE: a test context is STILL refused the production spine" {
  # No CHORUS_LOG_FILE override, so the surface resolves production — which is
  # exactly what must be refused. If this ever passes, #3615's teeth are gone.
  run env CI=true CHORUS_CONTEXT= BATS_TEST_TMPDIR="$BATS_TEST_TMPDIR" \
    CHORUS_LOG_FILE= "$CHORUS_LOG_BIN" test.scoped silas card=3918 marker=test-$$
  [[ "$output" == *"MEMBRANE REFUSED"* ]]
}

@test "werk.yml no longer clears CHORUS_CONTEXT for the werk-test RUNNER" {
  run grep -E '^\s+CHORUS_CONTEXT= .*werk-test' "${CHORUS_ROOT}/.github/workflows/werk.yml"
  [ "$status" -ne 0 ]
}
