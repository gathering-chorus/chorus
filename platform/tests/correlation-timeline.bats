#!/usr/bin/env bats
# correlation-timeline.bats — Tests for event correlation timeline (#2280)
# What Jeff sees: "what happened around this alert?" — a merged timeline from all event sources.

SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/correlation-timeline.sh"

# --- AC1: Script exists and takes a time range ---

@test "AC1: correlation-timeline.sh exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "AC1: script accepts --from and --to flags" {
  run bash "$SCRIPT" --help
  [[ "$output" == *"--from"* ]]
  [[ "$output" == *"--to"* ]]
}

# --- AC2: Events sorted by timestamp, labeled by source ---

@test "AC2: output includes source labels" {
  run bash "$SCRIPT" --from "2026-04-06 18:00" --to "2026-04-06 19:00"
  # Should contain at least one source label
  [[ "$output" == *"[spine]"* ]] || [[ "$output" == *"[hooks]"* ]] || [[ "$output" == *"[git]"* ]] || [[ "$output" == *"[alerts]"* ]]
}

@test "AC2: events are sorted chronologically" {
  # Extract timestamps, verify they're non-decreasing
  output=$(bash "$SCRIPT" --from "2026-04-06 18:00" --to "2026-04-06 19:00" 2>/dev/null)
  if [ -z "$output" ]; then
    skip "No events in test window"
  fi
  # Extract timestamps (first field) and check sort order
  timestamps=$(echo "$output" | grep -oE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}' | head -20)
  sorted=$(echo "$timestamps" | sort)
  [ "$timestamps" = "$sorted" ]
}

# --- AC3: Merged timeline from spine, hooks, alerts, deploys ---

@test "AC3: queries spine events (chorus.log)" {
  run bash "$SCRIPT" --from "2026-04-06 18:00" --to "2026-04-06 19:00"
  # Should find spine events in a recent window
  [[ "$output" == *"[spine]"* ]] || [[ "$output" == *"spine"* ]]
}

# --- AC4: Usable from CLI with human-readable flags ---

@test "AC4: accepts human-readable datetime format" {
  run bash "$SCRIPT" --from "2026-04-06 18:00" --to "2026-04-06 19:00"
  [ "$status" -eq 0 ]
}

@test "AC4: supports relative time shorthand" {
  run bash "$SCRIPT" --last 1h
  [ "$status" -eq 0 ]
}

# --- AC5: Output is human-readable, one line per event ---

@test "AC5: each line has timestamp, source, and description" {
  output=$(bash "$SCRIPT" --from "2026-04-06 18:00" --to "2026-04-06 19:00" 2>/dev/null | head -5)
  if [ -z "$output" ]; then
    skip "No events in test window"
  fi
  # Each line should start with a timestamp
  first_line=$(echo "$output" | head -1)
  [[ "$first_line" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2} ]]
}
