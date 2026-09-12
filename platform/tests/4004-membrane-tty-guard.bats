#!/usr/bin/env bats
# @test-type: unit — drives the grant check only; --dry-run keeps the body inert
#
# #4004 — test-product-membrane bootouts EVERY com.chorus.* agent, so it must
# never run unattended. Two inference-based guards failed in a row:
#   #3722 scanned ancestry for com.chorus. / the nightly runner (werk-test --nightly, #4145) — #3974 renamed
#   the runner to werk-test and the scan went silent, so the nightly booted
#   every agent and platform/api took 246 collateral failures.
#   Refusing without a controlling terminal — act allocates a pty, so a pipeline
#   run is indistinguishable from an operator. This card's own first test proved
#   it: green locally, red inside the pipeline.
# So the authority is now GRANTED, not inferred. A grant cannot be defeated by a
# rename or a pty.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/test-product-membrane.sh"

@test "NEGATIVE PROOF: without the grant it REFUSES with rc=3 and boots nothing" {
  # #4131 — the nightly exports NIGHTLY_UNIT_TIMEOUT to every unit, and under it
  # the membrane script is a freshness verdict, not a refusal. This proves the
  # grant check, so it brings its own env (#3528).
  run env -u NIGHTLY_UNIT_TIMEOUT -u WERK_TEST_NIGHTLY bash "$SCRIPT" --dry-run
  [ "$status" -eq 3 ]
  [[ "$output" == *"REFUSED"* ]]
  [[ "$output" == *"restore authority"* ]]
}

@test "NEGATIVE PROOF: a pty does NOT buy authority — act's pty must not pass" {
  if ! command -v script >/dev/null; then skip "no script(1) to allocate a pty"; fi
  # script(1) reports ITS own exit status, and mangles stderr framing, so assert
  # on what the guard actually does: nothing is booted and the body never runs.
  run script -q /dev/null bash "$SCRIPT" --dry-run
  [[ "$output" != *"would stop the above"* ]]
}

@test "with the explicit grant an operator CAN run it — the check separates its states" {
  run env MEMBRANE_ALLOW_UNDER_AGENT=1 bash "$SCRIPT" --dry-run
  [ "$status" -ne 3 ]
  [[ "$output" != *"REFUSED"* ]]
  [[ "$output" == *"dry-run"* ]]
}
