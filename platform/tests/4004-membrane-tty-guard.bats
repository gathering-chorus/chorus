#!/usr/bin/env bats
# @test-type: unit — drives the guard's two states directly; never runs the bootout body
#
# #4004 — test-product-membrane bootouts every com.chorus.* agent, so it must
# refuse when an automated runner is what would get booted. #3722 guarded that
# by scanning ancestry for "com.chorus." / "nightly-suites.sh". #3974 then moved
# suite execution into the werk-test binary: the parent became `werk-test`, the
# names stopped matching, the guard went quiet, and platform/api took 246
# collateral failures on the 2026-08-25 17:44 run. The replacement invariant is a
# controlling terminal, which no rename can take away.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/test-product-membrane.sh"

@test "NEGATIVE PROOF: no controlling terminal → REFUSES with rc=3, nothing booted" {
  # bats already gives us a non-tty stdin; this is the automated-runner state
  run bash "$SCRIPT" --dry-run
  [ "$status" -eq 3 ]
  [[ "$output" == *"REFUSED"* ]]
  [[ "$output" == *"no controlling terminal"* ]]
}

@test "an OPS run with a tty is NOT refused — the check separates its two states" {
  # A pty makes stdin a terminal without changing anything else. --dry-run keeps
  # the body inert, so this proves the GUARD passes, not that we booted anything.
  if ! command -v script >/dev/null; then skip "no script(1) to allocate a pty"; fi
  run script -q /dev/null bash "$SCRIPT" --dry-run
  [ "$status" -ne 3 ]
  [[ "$output" != *"no controlling terminal"* ]]
}

@test "the explicit override still works for an operator who owns the restore" {
  run env MEMBRANE_ALLOW_UNDER_AGENT=1 bash "$SCRIPT" --dry-run
  [ "$status" -ne 3 ]
  [[ "$output" != *"REFUSED"* ]]
}
