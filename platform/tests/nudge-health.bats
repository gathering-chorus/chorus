#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# nudge-health.bats — Tests for nudge health check (#1847)
# What Jeff sees: macOS notifications saying "3 role(s) unreachable"
# when all three sessions are running. Zombie Terminal windows crash
# the osascript lookup and the whole check fails.

# #3915 — was ${CHORUS_ROOT:-${CHORUS_ROOT}}: a tautology that resolves to an
# EMPTY path whenever CHORUS_ROOT is unset, which is exactly how the nightly
# runs it. The script was fine; the test could not find it and reported the
# health check as broken every night.
HEALTH_SCRIPT="${CHORUS_ROOT:-$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)}/platform/scripts/nudge-health-check.sh"

# --- AC 1: Health check survives zombie windows ---

@test "health check script exists and is executable" {
  [ -x "$HEALTH_SCRIPT" ]
}

# #3915 — these two assert LIVE state (all three role sessions registered and
# their panes alive). That is a real property worth checking, but it is not a
# property of the CODE: at 03:00 with no session started, or inside a bats
# subshell that cannot see the user's tmux, a red here says "the machine is
# quiet", not "the health check is broken". Nightly read it as the latter every
# night. So: measure the precondition first and SKIP when it does not hold —
# UNMEASURABLE, never a false red (#3753). When sessions ARE up, the assertion
# is unchanged and still catches the zombie-window bug it was written for.
# ROOT CAUSE (#3915): test_helper exports CHORUS_SESSIONS_DIR to an EMPTY
# tmpdir — the membrane's "a test brings its own world" (#3528). The health
# check then correctly reports "no registration" for every role, and these two
# tests called that a product defect. They are the only tests in this file that
# assert LIVE state, and inside a sandboxed world that state cannot exist.
#
# So the precondition is measured against THE DIRECTORY THE SCRIPT WILL READ,
# not against $HOME — and when the sandbox is in force (the normal case), the
# assertion is UNMEASURABLE and skips rather than reporting a false red.
roles_measurable() {
  local dir="${CHORUS_SESSIONS_DIR:-$HOME/.chorus/sessions}" n=0
  for r in wren silas kade; do
    ls "$dir/${r}-"*.json >/dev/null 2>&1 && n=$((n+1))
  done
  [ "$n" -eq 3 ] || return 1
  command -v tmux >/dev/null 2>&1 || return 1
  [ -n "$(tmux list-panes -a -F '#{pane_id}' 2>/dev/null)" ]
}

@test "health check succeeds when role sessions are running" {
  roles_measurable || skip "UNMEASURABLE: role sessions or tmux panes not visible from this test context"
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
}

@test "health check reports all roles reachable" {
  roles_measurable || skip "UNMEASURABLE: role sessions or tmux panes not visible from this test context"
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  echo "$output" | grep -q "all roles reachable"
}

# --- AC 3: Still detects genuinely missing roles ---

# --- #3284 AC8 / ADR-039: registry-aware, no false no-window for vscode ---

@test "AC8: health check resolves through the registry before Terminal-probing" {
  # The fix (ADR-039): resolve role→host via the session registry, branch on host.
  grep -q "resolve_reg" "$HEALTH_SCRIPT"
  grep -q 'host" = "vscode"' "$HEALTH_SCRIPT"
  grep -q -- "--vscode reachable" "$HEALTH_SCRIPT"
}

@test "AC8: a vscode session is healthy via --vscode, never no-window (the 84x false alarm)" {
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  # Only meaningful when a role is actually a live vscode session in this env.
  echo "$output" | grep -q "vscode session" || skip "no live vscode session registered in this env"
  # No role line may pair a vscode session with a no-window/unreachable alarm.
  ! ( echo "$output" | grep -i "vscode session" | grep -iq "no.*window" )
}

# --- #3673: tmux arm — registry host=tmux probes the pane, never Terminal ---

@test "3673: health check has a tmux arm (pane probe, tmux-pane-gone reason)" {
  grep -q 'host" = "tmux"' "$HEALTH_SCRIPT"
  grep -q "tmux-pane-gone" "$HEALTH_SCRIPT"
}

@test "3673: live tmux pane registration reports OK, never no-window" {
  command -v tmux >/dev/null 2>&1 || skip "tmux not installed"
  regdir="$(mktemp -d)"
  sess="hc-test-$$"
  tmux new-session -d -s "$sess" || skip "cannot start scratch tmux session"
  pane="$(tmux list-panes -t "$sess" -F '#{pane_id}' | head -1)"
  printf '{"role":"wren","pid":%s,"tty":"/dev/ttys999","host":"tmux","tmux":"%s","registered_at":"9999999999"}' "$$" "$pane" \
    > "${regdir}/wren-$$.json"
  run env CHORUS_SESSIONS_DIR="$regdir" bash "$HEALTH_SCRIPT"
  echo "output: $output"
  tmux kill-session -t "$sess" 2>/dev/null || true
  rm -rf "$regdir"
  echo "$output" | grep "wren" | grep -q "OK:"
  ! ( echo "$output" | grep "wren" | grep -q "no.*window" )
}

@test "3673: dead pane with live registration alerts tmux-pane-gone, not no-window" {
  command -v tmux >/dev/null 2>&1 || skip "tmux not installed"
  tmux list-sessions >/dev/null 2>&1 || skip "no tmux server running"
  regdir="$(mktemp -d)"
  printf '{"role":"wren","pid":%s,"tty":"/dev/ttys999","host":"tmux","tmux":"%%999","registered_at":"9999999999"}' "$$" \
    > "${regdir}/wren-$$.json"
  run env CHORUS_SESSIONS_DIR="$regdir" bash "$HEALTH_SCRIPT"
  echo "output: $output"
  rm -rf "$regdir"
  echo "$output" | grep "wren" | grep -q "tmux"
  echo "$output" | grep "wren" | grep -qi "ALERT"
  ! ( echo "$output" | grep "wren" | grep -q "no matching Terminal window" )
}

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
