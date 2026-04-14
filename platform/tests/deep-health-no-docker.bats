#!/usr/bin/env bats
# deep-health-no-docker.bats — verify deep-health doesn't check for Docker (#2020, #2032)
# Bug: deep-health checks for a chorus-hooks PID and compares binary timestamps.
# The shim is per-call, not a daemon. False positive every cycle.

DEEP_HEALTH="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/deep-health.sh"

@test "deep-health does not reference HOOKS_PID" {
  ! grep -q 'HOOKS_PID' "$DEEP_HEALTH"
}

@test "deep-health does not check binary mtime vs process start" {
  ! grep -q 'DISK_MTIME\|PROC_START\|binary.*newer' "$DEEP_HEALTH"
}

@test "deep-health checks shim binary exists instead" {
  grep -q 'SHIM_BIN\|chorus-hook-shim' "$DEEP_HEALTH"
}
