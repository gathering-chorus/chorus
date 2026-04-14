#!/usr/bin/env bats
# nudge-autosubmit.bats — verify nudge inject includes Return keystroke (#2029)
# What Jeff sees: nudge arrives in role terminal and auto-submits.
# Bug: #2245 switched to "do script" which doesn't send Return.
# Fix: revert to keystroke + key code 36.
# This is a structural test — the real test is Jeff watching the terminal.

INJECT_SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-inject/src/main.rs"

@test "inject source uses keystroke (not do script)" {
  grep -q 'keystroke' "$INJECT_SRC"
}

@test "inject source sends Return via key code 36" {
  grep -q 'key code 36' "$INJECT_SRC"
}

@test "inject source does NOT use do script for delivery" {
  ! grep -q 'do script.*in.*selected tab' "$INJECT_SRC"
}
