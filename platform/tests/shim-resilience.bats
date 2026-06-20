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

@test "wrapper emits clear error when binary missing" {
  CHORUS_ROOT=/nonexistent "$WRAPPER" 2>&1 | grep -q "chorus-hook-shim not found"
}

@test "wrapper logs failure to shim-wrapper.log" {
  CHORUS_ROOT=/nonexistent "$WRAPPER" 2>/dev/null || true
  grep -q "FATAL.*not found" ~/Library/Logs/Chorus/shim-wrapper.log
}
