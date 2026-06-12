#!/usr/bin/env bats
# 3379-lance-maintain.bats — Tests for #3379
# What Jeff sees: the API never wedges because lance fragments never regrow —
# maintainTable runs nightly OFF-PROCESS (the #3085 reindex-worker pattern),
# loudly, with deep-health watching its freshness. No human ever runs the
# compaction by hand again (2026-06-12: 35,032 fragments, 3 wedges, 2 manual
# kickstarts, one manual compaction under Jeff's go — that class dies here).

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO/platform/scripts/chorus-lance-maintain.sh"
WORKER="$REPO/platform/api/src/lance-maintain-worker.ts"
PLIST="$REPO/proving/config/launchagents/com.chorus.lance-maintain.plist"
DEEP_HEALTH="$REPO/platform/scripts/deep-health.sh"

@test "maintain script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "maintain script runs its own node process, never curls chorus-api" {
  # The whole point: synchronous/heavy lance work happens OFF the API's loop
  # (reindex-worker precedent, #3085). A curl to :3340 would re-import the bug.
  run grep -c "localhost:3340" "$SCRIPT"
  [ "$output" = "0" ]
  grep -q "dist/lance-maintain-worker.js" "$SCRIPT"
}

@test "maintain script pins node by absolute path (native-module ABI guard)" {
  # The #3085/#3086 lesson: PATH-resolved node found the wrong ABI and the
  # worker died silently for days. Pin it.
  grep -qE 'NODE_BIN=.*/\.nvm/versions/node/' "$SCRIPT"
}

@test "maintain script is lock-guarded with stale-lock recovery" {
  grep -q "LOCKFILE" "$SCRIPT"
  grep -qE "lock_age|stale" "$SCRIPT"
}

@test "maintain script logs where deep-health watches daily jobs" {
  # Loud-on-miss for free: deep-health's DAILY_LOGS freshness check (25h
  # threshold) covers any log in its LOG_DIR named in the list.
  grep -q "Library/Logs/Gathering/lance-maintain.log" "$SCRIPT"
  grep -q "lance-maintain.log" "$DEEP_HEALTH"
}

@test "worker source exists and uses the proven maintainTable" {
  [ -f "$WORKER" ]
  grep -q "maintainTable" "$WORKER"
  # Zero-row safety: the worker must refuse to call optimize on an empty/
  # unopenable table rather than 'maintain' a store that lost its data.
  grep -qE "countRows" "$WORKER"
}

@test "LaunchAgent plist exists with a nightly calendar schedule" {
  [ -f "$PLIST" ]
  grep -q "StartCalendarInterval" "$PLIST"
  grep -q "com.chorus.lance-maintain" "$PLIST"
}

@test "worker emits a spine event so the run is witnessed" {
  grep -qE "lance.maintain|chorus-log" "$SCRIPT"
}
