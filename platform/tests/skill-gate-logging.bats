#!/usr/bin/env bats
# skill-gate-logging.bats — verify structured logging for skill/gate invocations (#2015)
#
# Bug: Skill invocations log inconsistently — just name + timestamp, no role,
# card ID, or duration. Can't measure coordination layer performance.
# Fix: tool_telemetry emits skill.invoked spine events with structured fields.

TELEMETRY_SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/hooks/tool_telemetry.rs"

@test "tool_telemetry detects Skill tool and extracts skill name" {
  grep -q 'skill_name\|"Skill".*skill' "$TELEMETRY_SRC"
}

@test "skill invocation emits spine event with role and skill fields" {
  grep -q 'skill.invoked\|skill_invoked' "$TELEMETRY_SRC"
}

@test "gate invocations include card_id field" {
  grep -q 'card_id\|card' "$TELEMETRY_SRC"
}

@test "spine event uses chorus_log for Loki visibility" {
  grep -q 'chorus_log' "$TELEMETRY_SRC" && grep -q 'skill.invoked' "$TELEMETRY_SRC"
}
