#!/usr/bin/env bats
# @test-type: unit
# #3617 — alert-runner must honor rule schedules. Until now every alerts/*.yml
# ran on every runner cycle (only "manual" was honored), so the 8am-only
# fuseki-harvest rule fired at midnight and the 6-hourly lance rule joined the
# 00:00 battery. cron-due.py makes `schedule:` real. Deterministic: epochs are
# arguments; the matcher never reads the clock.

REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
DUE="$REPO_ROOT/proving/scripts/cron-due.py"

# #3665 — the epochs below are hand-computed in America/New_York, and cron-due.py
# evaluates cron fields in the process's local TZ (correct for prod: "8am harvest"
# means 8am Boston, and the runner is launchd-local). The TEST must pin that TZ or
# it fails on any non-Eastern box — exactly what happened on UTC CI (3 red under
# TZ=UTC, 0 under America/New_York, verified locally both ways).
export TZ=America/New_York

# 2026-07-16 08:00:00 EDT = epoch 1784203200
EIGHT_AM=1784203200
MIDNIGHT=1784174400   # 2026-07-16 00:00:00 EDT

@test "matcher exists and is executable" {
  [ -x "$DUE" ]
}

@test "every-minute rule is always due" {
  run "$DUE" "* * * * *" $((EIGHT_AM - 60)) "$EIGHT_AM"
  [ "$status" -eq 0 ]
}

@test "daily-8am rule is due when the window crosses 08:00" {
  run "$DUE" "0 8 * * *" $((EIGHT_AM - 300)) $((EIGHT_AM + 60))
  [ "$status" -eq 0 ]
}

@test "daily-8am rule is NOT due at midnight (the fuseki-harvest false-fire)" {
  run "$DUE" "0 8 * * *" $((MIDNIGHT - 300)) "$MIDNIGHT"
  [ "$status" -eq 1 ]
}

@test "6-hourly rule fires at 6h marks, not between" {
  run "$DUE" "0 */6 * * *" $((MIDNIGHT - 60)) "$MIDNIGHT"   # 00:00 IS a */6 mark
  [ "$status" -eq 0 ]
  run "$DUE" "0 */6 * * *" $((EIGHT_AM - 60)) "$EIGHT_AM"   # 08:00 is not
  [ "$status" -eq 1 ]
}

@test "never-ran (last=0) checks only the current minute" {
  run "$DUE" "0 8 * * *" 0 "$MIDNIGHT"
  [ "$status" -eq 1 ]
  run "$DUE" "0 8 * * *" 0 "$EIGHT_AM"
  [ "$status" -eq 0 ]
}

@test "malformed schedule fails open (rule still runs)" {
  run "$DUE" "whenever" $((EIGHT_AM - 60)) "$EIGHT_AM"
  [ "$status" -eq 0 ]
}

@test "stale last-run is capped: scan bounded to 24h, still terminates due" {
  run "$DUE" "0 8 * * *" $((EIGHT_AM - 864000)) $((EIGHT_AM + 60))
  [ "$status" -eq 0 ]
}
