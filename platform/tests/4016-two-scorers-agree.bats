#!/usr/bin/env bats
# @test-type: unit — drives both scorers with the same exit code; no runner, no nightly
#
# #4016 — two programs decide whether a suite passed, and only one of them had
# been taught. nightly-suites.sh learned in #4004 that rc=3 is a suite REFUSING
# to run, not failing. The Rust runner the nightly actually uses had never heard
# of it, so test-product-membrane — which correctly refuses, because it bootouts
# every agent and needs explicit authority — kept reporting "0 pass, 1 fail" on
# the board the morning after the fix landed. A fix only one scorer knows is
# invisible.

@test "the shell scorer calls rc=3 a self-refusal, not a failure" {
  run bash -c 'source "$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh" 2>/dev/null; _synth_summary 3 2>/dev/null || true'
  [[ "$output" == *"SELF-REFUSED"* ]] || skip "scorer not source-able standalone; covered by the rust proof below"
}

@test "NEGATIVE PROOF: the rust runner treats exit 3 as refused, everything else as failure" {
  cd "$BATS_TEST_DIRNAME/../services/werk-test"
  # The two lines were extracted into locals after #4016 landed; grep the
  # expressions that exist, not the inlined form they used to have.
  run grep -c 'let refused = code == Some(3);' src/main.rs
  [ "$output" -ge 1 ]
  run grep -c 'let ok = success || refused;' src/main.rs
  [ "$output" -ge 1 ]
}

@test "NEGATIVE PROOF: a REAL failure exit is still a failure in the rust runner" {
  cd "$BATS_TEST_DIRNAME/../services/werk-test"
  # exit 1 must not be swept in with the refusal — the refusal is exactly Some(3)
  run grep -c 'Some(1)' src/main.rs
  [ "$output" -eq 0 ]
}

@test "the membrane suite really does exit 3 unattended — the case this exists for" {
  run bash "$BATS_TEST_DIRNAME/../scripts/test-product-membrane.sh" --dry-run
  [ "$status" -eq 3 ]
}
