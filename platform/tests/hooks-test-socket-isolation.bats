#!/usr/bin/env bats
# @test-type: integration:security — spawns a real daemon beside a stand-in prod agent socket
# @domain: services — the product domain this suite guards (#4334)
# #4849 — a test chorus-hooks daemon must never take the prod agent socket.
#
# 11-01 03:41: the nightly ran 4025-hooks-terminating-witness.bats. Its daemon had
# its own HOME but inherited CHORUS_STATE_DIR=~/.chorus, so it resolved its run
# dir to ~/.chorus/run-agents and bound its 0600 control socket over the live
# 0660 agent socket. Every role got "connect: Permission denied (os error 13)"
# until Jeff kickstarted com.chorus.hooks at 05:27.
#
# Here a short private dir plays prod's state dir: it holds a live listener at
# run-agents/chorus-hooks.sock, mode 0660, the way prod's daemon leaves it. The
# daemon under test gets its own HOME and CHORUS_STATE_DIR=<that stand-in>.

TREE="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
BIN="${CHORUS_HOOKS_BIN:-$TREE/platform/services/chorus-hooks/target/release/chorus-hooks}"

setup() {
  export HOME="$(mktemp -d /tmp/h4849.XXXXXX)"
  PROD="$(mktemp -d /tmp/p4849.XXXXXX)"
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  : > "$CHORUS_LOG_FILE"
  export CHORUS_CONTEXT=test
  export CHORUS_ROOT="$TREE"
  mkdir -p "$PROD/run-agents"
  AGENT_SOCK="$PROD/run-agents/chorus-hooks.sock"
}

teardown() {
  [[ -n "${DPID:-}" ]] && kill -KILL "$DPID" 2>/dev/null || true
  [[ -n "${LPID:-}" ]] && kill -KILL "$LPID" 2>/dev/null || true
  [[ "$HOME" == /tmp/h4849.* ]] && rm -rf "$HOME"
  [[ "${PROD:-}" == /tmp/p4849.* ]] && rm -rf "$PROD"
}

# The stand-in prod daemon: a listener that stays up, socket mode 0660.
prod_listener() {
  python3 -c '
import socket, os, sys, time
s = socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); os.chmod(sys.argv[1], 0o660); s.listen(8)
while True: time.sleep(1)' "$AGENT_SOCK" &
  LPID=$!
  for _ in $(seq 1 50); do [[ -S "$AGENT_SOCK" ]] && break; sleep 0.1; done
  [[ -S "$AGENT_SOCK" ]] || return 1
}

# Every .bats under $1 that starts the daemon binary without its own HOME (the
# daemon's run dir follows HOME; CHORUS_HOOKS_RUN_DIR from the env is ignored).
leaky_daemon_tests() {
  local f
  for f in "$1"/*.bats; do
    grep -q 'chorus-hooks/target/release/chorus-hooks' "$f" || continue
    grep -qE 'export HOME=|<key>HOME</key>' "$f" || basename "$f"
  done
}

sock_id() { stat -f '%i %Lp' "$1" 2>/dev/null; }

# The check: is the agent socket still prod's? Same inode, mode 660, and a
# connect is answered.
agent_socket_is_prods() {
  [[ "$(sock_id "$AGENT_SOCK")" == "$1" ]] || { echo "agent socket changed: was '$1', now '$(sock_id "$AGENT_SOCK")'"; return 1; }
  python3 -c 'import socket,sys; socket.socket(socket.AF_UNIX).connect(sys.argv[1])' "$AGENT_SOCK" \
    || { echo "agent socket no longer answers"; return 1; }
}

start_daemon() {
  # the env werk-test (#4785) and env-setup hand every suite
  CHORUS_STATE_DIR="$PROD" CHORUS_HOOKS_RUN_DIR="$PROD/run-agents" "$BIN" >"$BATS_TEST_TMPDIR/daemon.out" 2>&1 &
  DPID=$!
  for _ in $(seq 1 50); do
    grep -q '"event":"service.started"' "$CHORUS_LOG_FILE" && return 0
    kill -0 "$DPID" 2>/dev/null || break
    sleep 0.1
  done
  echo "daemon never came up:"; cat "$BATS_TEST_TMPDIR/daemon.out"
  return 1
}

@test "AC1/AC4: a daemon started in a role's env leaves prod's agent socket and dir alone" {
  [[ -x "$BIN" ]] || { echo "release binary not built: $BIN"; return 1; }
  prod_listener
  chmod 0710 "$PROD/run-agents"
  before="$(sock_id "$AGENT_SOCK")"
  start_daemon
  sleep 0.5
  agent_socket_is_prods "$before"
  [[ "$(stat -f %Lp "$PROD/run-agents")" == 710 ]] || { echo "agent dir mode changed to $(stat -f %Lp "$PROD/run-agents")"; return 1; }
  # and the daemon is in its own world
  [[ -S "$HOME/.chorus/run/chorus-hooks.sock" ]] || { echo "daemon socket not under its HOME"; cat "$BATS_TEST_TMPDIR/daemon.out"; return 1; }
  kill -TERM "$DPID"; sleep 0.5
  agent_socket_is_prods "$before"
}

@test "AC3 NEGATIVE PROOF: the check goes red when the agent socket is taken over" {
  prod_listener
  before="$(sock_id "$AGENT_SOCK")"
  # what the 03:41 daemon did: remove it and bind its own, 0600
  rm -f "$AGENT_SOCK"
  python3 -c 'import socket,os,sys; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); os.chmod(sys.argv[1],0o600)' "$AGENT_SOCK"
  run agent_socket_is_prods "$before"
  [ "$status" -ne 0 ]
  [[ "$output" == *"agent socket changed"* ]] || return 1
}

@test "AC1: the daemon refuses to take an agent socket another daemon is serving" {
  [[ -x "$BIN" ]] || { echo "release binary not built: $BIN"; return 1; }
  # A daemon whose own agent dir IS the live one (HOME points at prod's parent).
  mkdir -p "$HOME/.chorus/run-agents"
  AGENT_SOCK="$HOME/.chorus/run-agents/chorus-hooks.sock"
  dscl . -read /Groups/chorus PrimaryGroupID >/dev/null 2>&1 || skip "no chorus group on this host"
  prod_listener
  before="$(sock_id "$AGENT_SOCK")"
  CHORUS_HOOKS_RUN_DIR="$HOME/.chorus/run" "$BIN" >"$BATS_TEST_TMPDIR/daemon.out" 2>&1 &
  DPID=$!
  for _ in $(seq 1 50); do grep -q 'agent socket' "$BATS_TEST_TMPDIR/daemon.out" && break; sleep 0.1; done
  grep -q 'is live under another daemon' "$BATS_TEST_TMPDIR/daemon.out" || { cat "$BATS_TEST_TMPDIR/daemon.out"; return 1; }
  agent_socket_is_prods "$before"
}

@test "AC2: every bats that starts the chorus-hooks daemon gives it its own HOME" {
  bad="$(leaky_daemon_tests "$TREE/platform/tests")"
  [[ -z "$bad" ]] || { echo "daemon tests without their own world: $bad"; return 1; }
}

@test "AC2 NEGATIVE PROOF: the guard names a daemon test that has no world of its own" {
  fx="$BATS_TEST_TMPDIR/tests"; mkdir -p "$fx"
  printf 'BIN=x/chorus-hooks/target/release/chorus-hooks\n"$BIN" &\n' > "$fx/leaky.bats"
  [[ "$(leaky_daemon_tests "$fx")" == "leaky.bats" ]] || return 1
}
