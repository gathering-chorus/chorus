#!/usr/bin/env bats
# @test-type: integration:security — spawns a real daemon in its own world and signals it
# #4025 — chorus-hooks names its terminator.
#
# Three exit -15 deaths in 24h with no record of who sent the signal. The fix
# arms an SA_SIGINFO witness before tokio's handler and emits `hooks.terminating`
# with signal + sender pid/uid/comm + ppid before cleanup.
#
# Every test brings its own world (#3528): HOME=$BATS_TEST_TMPDIR gives the
# daemon its own ~/.chorus/run socket + pidfile; CHORUS_LOG_FILE gives it its
# own spine. Nothing here touches the live daemon or the live spine.
#
# AC1 — SIGTERM emits hooks.terminating with signal + sender pid (test 2)
# AC2 — NEGATIVE PROOF, both halves:
#         graceful terminate  → event PRESENT, sender = this test's shell
#         SIGKILL (uncatchable) → event ABSENT, stated honestly (test 3)
#       plus: the pid field must be able to be WRONG (test 4) — a witness that
#       reports the sender it was told, not the sender it saw, is no witness.

# The binary under test is THIS tree's build, never $CHORUS_ROOT's — a role
# session exports CHORUS_ROOT=canonical, and canonical's target/release is the
# OLD daemon (landed ≠ running: the first run of this file "failed" green-ly
# against a binary that had no witness at all).
TREE="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
BIN="$TREE/platform/services/chorus-hooks/target/release/chorus-hooks"

setup() {
  [[ -x "$BIN" ]] || skip "release binary not built: $BIN"
  # the socket path must fit SUN_LEN (104 bytes on macOS) — $BATS_TEST_TMPDIR
  # under /var/folders is too long, so HOME gets a short private dir instead.
  export HOME="$(mktemp -d /tmp/h4025.XXXXXX)"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  : > "$CHORUS_LOG_FILE"
  export CHORUS_CONTEXT=test
  export CHORUS_ROOT="$TREE"
}

start_daemon() {
  "$BIN" >"$BATS_TEST_TMPDIR/daemon.out" 2>&1 &
  DPID=$!
  local sock="$HOME/.chorus/run/chorus-hooks.sock"
  for _ in $(seq 1 50); do
    [[ -S "$sock" ]] && grep -q '"event":"hooks.started"' "$CHORUS_LOG_FILE" && return 0
    sleep 0.1
  done
  echo "daemon never came up:"; cat "$BATS_TEST_TMPDIR/daemon.out"
  echo "spine ($CHORUS_LOG_FILE):"; cat "$CHORUS_LOG_FILE"
  return 1
}

wait_gone() {
  for _ in $(seq 1 50); do
    kill -0 "$1" 2>/dev/null || return 0
    sleep 0.1
  done
  return 1
}

teardown() {
  [[ -n "${DPID:-}" ]] && kill -KILL "$DPID" 2>/dev/null || true
  [[ "$HOME" == /tmp/h4025.* ]] && rm -rf "$HOME"
}

@test "hooks.started lands on the daemon's own spine with pid + ppid" {
  start_daemon
  line=$(grep '"event":"hooks.started"' "$CHORUS_LOG_FILE" | tail -1)
  [[ "$line" == *"\"pid\":\"$DPID\""* ]]
  [[ "$line" == *"\"ppid\":\"$$\""* ]]
  [[ "$line" == *'"role":"system"'* ]]
}

@test "AC1: SIGTERM → hooks.terminating names signal + sender pid/uid/comm + ppid" {
  start_daemon
  kill -TERM "$DPID"
  wait_gone "$DPID"
  line=$(grep '"event":"hooks.terminating"' "$CHORUS_LOG_FILE" | tail -1)
  echo "line=$line"
  [[ -n "$line" ]]
  [[ "$line" == *'"signal":"SIGTERM"'* ]]
  # the sender is THIS bats shell — the witness saw si_pid, not a guess
  [[ "$line" == *"\"sender_pid\":\"$$\""* ]]
  [[ "$line" == *"\"sender_uid\":\"$(id -u)\""* ]]
  [[ "$line" == *'"sender_comm":"'* ]]
  [[ "$line" != *'"sender_comm":"unknown"'* ]]
  [[ "$line" == *"\"ppid\":\"$$\""* ]]
  [[ "$line" == *'"uptime_s":"'* ]]
}

@test "AC2 NEGATIVE PROOF: SIGKILL is uncatchable → NO hooks.terminating (absence stated honestly)" {
  start_daemon
  kill -KILL "$DPID"
  wait_gone "$DPID"
  # give a would-be emit every chance to land before asserting absence
  sleep 0.5
  run grep -c '"event":"hooks.terminating"' "$CHORUS_LOG_FILE"
  echo "count=$output"
  [[ "$output" == "0" ]]
  # the daemon's own record ends at hooks.started — that gap IS the evidence
  grep -q '"event":"hooks.started"' "$CHORUS_LOG_FILE"
}

@test "AC2 NEGATIVE PROOF: the sender field is the OBSERVED sender — a different killer reads differently" {
  start_daemon
  # kill from a subshell: si_pid is the subshell, not this test's shell.
  ( kill -TERM "$DPID" )
  wait_gone "$DPID"
  line=$(grep '"event":"hooks.terminating"' "$CHORUS_LOG_FILE" | tail -1)
  echo "line=$line"
  [[ -n "$line" ]]
  # if the witness were just echoing getppid() the two would collide; they must not
  [[ "$line" != *"\"sender_pid\":\"$$\""* ]]
  [[ "$line" == *"\"ppid\":\"$$\""* ]]
  # the subshell exited the moment kill returned — the name resolves as gone,
  # which is exactly the state a one-shot terminator leaves behind in prod
  [[ "$line" == *'"sender_comm":"gone"'* ]]
}
