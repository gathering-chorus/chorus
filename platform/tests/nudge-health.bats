#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
# @domain: messages — the product domain this suite guards (#4334)
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

# #4130 — the last test scripts Terminal.app through osascript. Under the 03:00
# nightly (launchd, no foreground GUI session) that AppleEvent sat 481s and came
# back as a timeout error, not a window count — a 5-pass file called red by one
# hung call. Measure the precondition against the same app the test will ask,
# bounded: if Terminal cannot answer a trivial query in 10s, the assertion is
# UNMEASURABLE from this context. Where a Terminal is scriptable it runs as before.
terminal_scriptable() {
  perl -e 'alarm 10; exec @ARGV' osascript -e 'tell application "Terminal" to count windows' >/dev/null 2>&1
}

# #4336 — a hermetic world for running the REAL script. It hardcodes
# CANARY_DIR=/tmp/nudge-canary and POSTs an alert to the Clearing (3470) on any
# failure, so each run uses a copy with the canary dir pointed into the test's
# tmpdir (the copy is checked to differ — a silent no-op sed would fall back to
# /tmp), a curl stub that records instead of posting, and a CHORUS_ROOT whose
# chorus-log is a recorder. osascript/pgrep/tmux are stubbed only when a case
# asks (STUB_OSA / STUB_PGREP_RC / STUB_TMUX_PANES); otherwise the real ones run.
hc_world() {
  W="$BATS_TEST_TMPDIR/hc"
  mkdir -p "$W/bin" "$W/root/platform/scripts" "$W/sessions" "$W/canary"
  sed "s|^CANARY_DIR=\"/tmp/nudge-canary\"|CANARY_DIR=\"$W/canary\"|" "$HEALTH_SCRIPT" > "$W/health.sh"
  ! cmp -s "$W/health.sh" "$HEALTH_SCRIPT" || { echo "canary redirect did not apply" >&2; return 1; }
  printf '#!/bin/bash\necho "$*" >> "%s/curl.calls"\n' "$W" > "$W/bin/curl"
  printf '#!/bin/bash\necho "$*" >> "%s/spine.calls"\n' "$W" > "$W/root/platform/scripts/chorus-log"
  if [ -n "${STUB_OSA:-}" ]; then
    # records the AppleScript it was asked to run; answers "<count>::<name>"
    printf '#!/bin/bash\necho "$*" | tr "\\n" " " >> "%s/osa.calls"; echo >> "%s/osa.calls"\necho "%s"\n' "$W" "$W" "$STUB_OSA" > "$W/bin/osascript"
  fi
  if [ -n "${STUB_PGREP_RC:-}" ]; then
    printf '#!/bin/bash\nexit %s\n' "$STUB_PGREP_RC" > "$W/bin/pgrep"
  fi
  if [ -n "${STUB_TMUX_PANES:-}" ]; then
    printf '#!/bin/bash\n[ "$1" = list-panes ] && printf "%%s\\n" %s\nexit 0\n' "$STUB_TMUX_PANES" > "$W/bin/tmux"
  fi
  chmod +x "$W/bin/"* "$W/root/platform/scripts/chorus-log"
}

# register <role> <host> [pane] — a live registration (pid = this test process)
register() {
  printf '{"role":"%s","pid":%s,"tty":"/dev/ttys999","host":"%s","tmux":"%s","registered_at":"9999999999"}' \
    "$1" "$$" "$2" "${3:-}" > "$W/sessions/$1-$$.json"
}

run_health() {
  run env PATH="$W/bin:$PATH" CHORUS_ROOT="$W/root" CHORUS_SESSIONS_DIR="$W/sessions" bash "$W/health.sh"
  echo "output: $output"
}

@test "health check succeeds when role sessions are running" {
  # #4336 — reads the LIVE registry and probes the real Terminal; on a failure the
  # script POSTs to the Clearing. Live-box only, never in a hermetic run.
  [ "${RUN_LIVE_INTEGRATION:-}" = "true" ] || skip "UNMEASURED — asserts live role sessions + real Terminal, and alerts the Clearing on failure (#4336)"
  roles_measurable || skip "UNMEASURABLE: role sessions or tmux panes not visible from this test context"
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
}

@test "health check reports all roles reachable" {
  # #4336 — reads the LIVE registry and probes the real Terminal; on a failure the
  # script POSTs to the Clearing. Live-box only, never in a hermetic run.
  [ "${RUN_LIVE_INTEGRATION:-}" = "true" ] || skip "UNMEASURED — asserts live role sessions + real Terminal, and alerts the Clearing on failure (#4336)"
  roles_measurable || skip "UNMEASURABLE: role sessions or tmux panes not visible from this test context"
  run bash "$HEALTH_SCRIPT"
  echo "output: $output"
  echo "$output" | grep -q "all roles reachable"
}

# --- AC 3: Still detects genuinely missing roles ---

# --- #3284 AC8 / ADR-039: registry-aware, no false no-window for vscode ---

# #4336 — was three greps of the script text (resolve_reg, host = vscode,
# "--vscode reachable"). Now: a registered vscode session is run through the
# script with Terminal stubbed; it must be judged by the Code-app check and
# never handed to the Terminal window probe.
@test "AC8: a registered vscode session resolves through the registry, not the Terminal probe" {
  STUB_OSA="1::only claude" STUB_PGREP_RC=0 hc_world
  register wren vscode
  run_health
  echo "$output" | grep "wren" | grep -q "OK: wren — vscode session (pid $$) alive, Code running → --vscode reachable"
  # the Terminal probe ran for the unregistered roles, never for wren
  [ -s "$W/osa.calls" ] || return 1
  ! grep -q '"wren"' "$W/osa.calls" || return 1
}

@test "AC8: a vscode session with Code down alerts vscode-app-down, never no-window (the 84x false alarm)" {
  STUB_OSA="0::" STUB_PGREP_RC=1 hc_world
  register wren vscode
  run_health
  [ "$status" -eq 1 ]
  echo "$output" | grep "wren" | grep -q "ALERT: wren — vscode session (pid $$) registered but Code app is NOT running"
  ! ( echo "$output" | grep "wren" | grep -iq "no.*window" ) || return 1
  grep -q "role=wren,reason=vscode-app-down" "$W/spine.calls"
  ! grep -q "role=wren,reason=no-window" "$W/spine.calls" || return 1
}

# --- #3673: tmux arm — registry host=tmux probes the pane, never Terminal ---

# #4336 — was two greps (host = tmux, "tmux-pane-gone"). Now the tmux arm is
# run with a stubbed tmux server: a registered pane that exists is OK, one that
# does not emits reason=tmux-pane-gone — and Terminal is never probed for it.
@test "3673: the tmux arm probes the pane and reports tmux-pane-gone (stubbed tmux, script run)" {
  STUB_OSA="0::" STUB_TMUX_PANES="%42" hc_world
  register wren tmux %42
  register silas tmux %99
  run_health
  echo "$output" | grep "wren" | grep -q "OK: wren — tmux session (pid $$) alive, pane %42 exists"
  echo "$output" | grep "silas" | grep -q "ALERT: silas — tmux session (pid $$) registered but pane %99 NOT found"
  grep -q "role=silas,reason=tmux-pane-gone,pid=$$,pane=%99" "$W/spine.calls"
  ! grep -q '"wren"' "$W/osa.calls" || return 1
  ! grep -q '"silas"' "$W/osa.calls" || return 1
}

@test "3673: live tmux pane registration reports OK, never no-window" {
  command -v tmux >/dev/null 2>&1 || skip "tmux not installed"
  hc_world
  sess="hc-test-$$"
  tmux new-session -d -s "$sess" || skip "cannot start scratch tmux session"
  pane="$(tmux list-panes -t "$sess" -F '#{pane_id}' | head -1)"
  register wren tmux "$pane"
  run_health
  tmux kill-session -t "$sess" 2>/dev/null || true
  echo "$output" | grep "wren" | grep -q "OK:"
  ! ( echo "$output" | grep "wren" | grep -q "no.*window" ) || return 1
}

@test "3673: dead pane with live registration alerts tmux-pane-gone, not no-window" {
  command -v tmux >/dev/null 2>&1 || skip "tmux not installed"
  tmux list-sessions >/dev/null 2>&1 || skip "no tmux server running"
  hc_world
  register wren tmux "%999"
  run_health
  echo "$output" | grep "wren" | grep -q "tmux"
  echo "$output" | grep "wren" | grep -qi "ALERT"
  ! ( echo "$output" | grep "wren" | grep -qi "no matching Terminal window" ) || return 1
}

# #4336 — the old case ran its own copy of the AppleScript and never called the
# script. Now the script runs with Terminal answering "no matching window":
# every unregistered role must alert, name no-window, and the check must exit 1.
@test "health check detects missing role window (script run, Terminal stubbed)" {
  STUB_OSA="0::" hc_world
  run_health
  [ "$status" -eq 1 ]
  for r in wren silas kade; do
    echo "$output" | grep -q "ALERT: $r — no registration AND no Terminal window" || return 1
    grep -q "role=$r,reason=no-window,host=none" "$W/spine.calls" || return 1
  done
  echo "$output" | grep -q "NUDGE HEALTH: 3 role(s) have issues"
  [ -s "$W/curl.calls" ] || return 1
}

@test "NEGATIVE: one matching window per role reads healthy (the missing-window alarm is not constant)" {
  STUB_OSA="1::only claude" hc_world
  run_health
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "NUDGE HEALTH: all roles reachable"
  [ ! -e "$W/curl.calls" ] || return 1
}
