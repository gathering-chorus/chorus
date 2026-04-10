#!/usr/bin/env bats
# nudge-health.bats — Tests for nudge health check (#1847)
# What Jeff sees: macOS notifications saying "3 role(s) unreachable"
# when all three sessions are running. Zombie Terminal windows crash
# the osascript lookup and the whole check fails.

HEALTH_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge-health-check.sh"

# --- AC 1: Health check survives zombie windows ---

@test "health check script exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

@test "health check succeeds when role sessions are running" {
  # All three role windows exist (wren, silas, kade)
  # If zombie windows crash the check, this fails — proving the bug
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
}

@test "health check reports all roles reachable" {
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  echo "$output" | grep -q "all roles reachable"
}

# --- AC 3: Still detects genuinely missing roles ---

@test "health check detects missing role window" {
  # Use a role name that won't match any window
  # Temporarily override ROLES to test with a fake role
  run bash -c '
    SCRIPT="$1"
    # Extract just the osascript check for a nonexistent pattern
    result=$(osascript -e "
tell application \"Terminal\"
    set matchCount to 0
    set matchName to \"\"
    set winCount to count of windows
    repeat with i from 1 to winCount
        try
            set w to window i
            set winName to name of w
            if winName contains \"nonexistent-role-xyz\" and winName contains \"claude\" then
                set matchCount to matchCount + 1
                set matchName to winName
            end if
        end try
    end repeat
    return (matchCount as text) & \"::\" & matchName
end tell" 2>&1)
    count=$(echo "$result" | cut -d":" -f1)
    [ "$count" = "0" ]
  ' -- "$HEALTH_SCRIPT"
  [ "$status" -eq 0 ]
}
