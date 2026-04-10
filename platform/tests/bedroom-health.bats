#!/usr/bin/env bats
# bedroom-health.bats — Tests for #1853
# What Jeff sees: Bedroom disk/memory issues go undetected because health check only runs manually

HEALTH_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/health-check-bedroom.sh"
PLIST_DIR="/Users/jeffbridwell/CascadeProjects/chorus/proving/config/launchagents"

@test "health-check-bedroom.sh exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

@test "health check nudges silas on failures" {
  grep -q 'NUDGE.*silas\|nudge.*silas\|"$NUDGE" silas' "$HEALTH_SCRIPT"
}

@test "health check has hourly cooldown on nudges" {
  grep -q "COOLDOWN\|cooldown" "$HEALTH_SCRIPT"
}

@test "LaunchAgent plist exists for bedroom health check" {
  [ -f "$PLIST_DIR/com.chorus.bedroom-health.plist" ]
}
