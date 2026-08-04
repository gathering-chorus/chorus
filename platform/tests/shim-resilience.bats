#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# shim-resilience.bats — verify shim wrapper graceful degradation (#2034)

SCRIPTS="${CHORUS_ROOT}/platform/scripts"
WRAPPER="$SCRIPTS/shim-wrapper.sh"

@test "shim-wrapper.sh exists and is executable" {
  [ -x "$WRAPPER" ]
}

@test "all 17 shim scripts symlink to shim-wrapper.sh" {
  count=0
  for name in chorus-log role-state wall-clock heartbeat chorus-init-db \
    claudemd-gen context-cache-5min context-cache-daily context-cache-hourly \
    context-cache-weekly cruft-scan log-rotate role-checkpoint \
    session-close-thin session-end-hook session-start-thin workflow; do
    target=$(readlink "$SCRIPTS/$name" 2>/dev/null)
    [ "$target" = "shim-wrapper.sh" ]
    count=$((count + 1))
  done
  [ "$count" -eq 17 ]
}

@test "wrapper emits clear error when binary missing" {
  # #3606 — this could not reach the state it tests. It cleared PATH and
  # CHORUS_ROOT but not the wrapper's SECOND fallback,
  # $HOME/.chorus/bin/chorus-hook-shim (#2734's deploy location), which exists on
  # every machine where the binary has been deployed. So the wrapper found the
  # shim, ran it, exited 0 with no output — and the assertion failed against a
  # working wrapper. It could only have passed on a box with nothing deployed.
  # Supply a HOME with no .chorus/bin, the way the sibling test below already
  # supplies one for the log path.
  local home="${BATS_TEST_TMPDIR:-/tmp}/shim-nobin-$$"
  mkdir -p "$home/Library/Logs/Chorus"
  run env HOME="$home" CHORUS_ROOT=/nonexistent PATH=/usr/bin:/bin "$WRAPPER"
  [[ "$output" == *"chorus-hook-shim not found"* ]]
}

@test "wrapper logs failure to shim-wrapper.log" {
  # The wrapper appends to $HOME/Library/Logs/Chorus/ and does not create that
  # directory, so the test supplies it instead of assuming this machine has one.
  local home="${BATS_TEST_TMPDIR:-/tmp}/shim-home-$$"
  mkdir -p "$home/Library/Logs/Chorus"
  run env HOME="$home" CHORUS_ROOT=/nonexistent PATH=/usr/bin:/bin "$WRAPPER"
  grep -q "FATAL.*not found" "$home/Library/Logs/Chorus/shim-wrapper.log"
}
