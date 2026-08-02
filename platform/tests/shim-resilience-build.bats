#!/usr/bin/env bats
# @test-type: integration — needs the built chorus-hook-shim binary
# #3721 — split out of shim-resilience.bats. That file's other cases are true
# hermetic source guards (the symlink map, the wrapper's error text, its log
# line) and pass on a runner. This one EXECUTES a shim script end-to-end, which
# resolves `command -v chorus-hook-shim` and falls back to
# target/release/chorus-hook-shim — neither exists in the bats CI job, which
# installs no Rust toolchain and builds nothing. @test-type is per-FILE, so the
# honest split is a second file rather than dragging four working hermetic
# asserts out of the unit tier to accommodate one that cannot be hermetic.
# The local nightly runs on the machine where the binary is deployed, which is
# where "does the wrapper actually exec the shim" is a real question.
load test_helper

SCRIPTS="${CHORUS_ROOT}/platform/scripts"

@test "wall-clock works through wrapper" {
  result=$("$SCRIPTS/wall-clock" 2>&1)
  echo "$result" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}'
}

# #3710 — both cases set CHORUS_ROOT=/nonexistent and expected the not-found
# branch. That stopped working when #2734 moved the deploy location to
# ~/.chorus/bin: the wrapper resolves `command -v chorus-hook-shim` FIRST and
# only falls back to $CHORUS_ROOT, so on any machine with the binary on PATH it
# found the real shim and never reached the error branch. Clearing PATH too is
# what actually exercises "binary missing" now.
