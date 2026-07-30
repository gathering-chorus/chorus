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
@test "wrapper emits clear error when binary missing" {
  run env CHORUS_ROOT=/nonexistent PATH=/usr/bin:/bin "$WRAPPER"
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
