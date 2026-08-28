#!/usr/bin/env bats
# @test-type: integration — runs the live chorus-health (needs chorus-api :3340); skip-if-absent
# #4027 — chorus-health called platform/tests/owl-api-drift-check.sh and
# owl-api-conformance.sh for six days after #3561 renamed them to athena-make-*.
# bash's "No such file" was read as a failed check and reported as drift
# 600+ times. Negative proof per #3734: with the check scripts ABSENT the
# health run must FAIL under its own name (owl-api-checks-missing), and must
# NOT report owl-api-drift / owl-api-conformance — a missing monitor is not a
# finding about the thing it monitors.

HEALTH="$BATS_TEST_DIRNAME/../scripts/chorus-health"

setup() {
  export CHORUS_HEALTH_NO_EMIT=1   # a test FAIL must not become a live chorus.health event/nudge
  curl -s -o /dev/null --max-time 3 http://localhost:3340/api/chorus/health || skip "chorus-api not running"
  curl -s -o /dev/null --max-time 3 http://localhost:3360/health || skip "athena-make not running (the branch under test is only reached when :3360 answers)"
}

@test "NEGATIVE PROOF: check scripts absent → FAIL owl-api-checks-missing, never reported as drift" {
  CHORUS_TESTS_DIR="$BATS_TEST_TMPDIR/no-such-tests" run bash "$HEALTH" -v
  [ "$status" -eq 1 ]
  [[ "$output" == *"owl-api-checks-missing"* ]]
  [[ "$output" != *"owl-api-drift:"* ]]
  [[ "$output" != *"owl-api-conformance:"* ]]
}

@test "with the real scripts present the missing-check state does not fire" {
  run bash "$HEALTH" -v
  [[ "$output" != *"owl-api-checks-missing"* ]]
  [[ "$output" == *"owl-api-drift"* ]] || [[ "$output" == *"owl-api-conformance"* ]]
}
