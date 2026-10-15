#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs (tmux, claude, token, curl); no live services
# @domain: identity
#
# #4368 — login reads the Principal row first. Its kind decides: an agent logs
# in; a service or a person is refused with why; no row is refused naming the
# census; an identity API that does not answer is refused with one next
# command. The list of three names in the code is gone: an agent the code has
# never heard of logs in, and that case fails if the list comes back.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}
row() { printf '{"data":{"principalKind":"%s"}}\n200\n' "$2" > "$T/principal-$1.json"; }
out_has() { printf '%s' "$output" | grep -qF -- "$1"; }
nothing_started() { test -z "$(grep -F send-keys "$T/tmux.log" 2>/dev/null || true)"; test ! -f "$T/token.log"; }

@test "#4368 an agent's row lets it log in, and the row was read" {
  run "$SCRIPT" login wren
  test "$status" -eq 0
  grep -q "/v1/identity/principals/wren" "$T/curl.log"
  grep -q "send-keys -t chorus-wren" "$T/tmux.log"
}

@test "#4368 a service is refused with why, and nothing starts" {
  row bridge service
  mkdir -p "$T/roles/bridge"
  run "$SCRIPT" login bridge
  test "$status" -eq 2
  out_has "bridge is a service principal; it acts with its credential, it does not log in"
  nothing_started
}

@test "#4368 NEGATIVE PROOF: a service with a role home is still refused (the kind decides, not the folder)" {
  row wren service
  run "$SCRIPT" login wren
  test "$status" -eq 2
  out_has "wren is a service principal"
  nothing_started
}

@test "#4368 a person is refused: a person attends, never logs in" {
  row jeff person
  mkdir -p "$T/roles/jeff"
  run "$SCRIPT" login jeff
  test "$status" -eq 2
  out_has "jeff is a person; a person attends a role's session and never logs in"
  nothing_started
}

@test "#4368 no row is refused, naming the census" {
  run "$SCRIPT" login bob
  test "$status" -eq 2
  out_has "no Principal row named bob. Who exists: chorus-principal census"
  nothing_started
}

@test "#4368 identity API down is refused with one next command, never a guess" {
  printf '\n000\n' > "$T/principal-wren.json"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  out_has "could not read wren's Principal row"
  out_has "Next: agent-state.sh restart athena-make"
  nothing_started
}

# #4532 reopen 1 — this case used abby-normal until #4444 put her on the agy
# runtime (shared/agent_runtimes.json names her), so she stopped being an agent
# the code never named and the nightly went red on a pane that never showed
# agy (10-15 05:29). The name here must stay in no list and no runtime table.
@test "#4368 the list of three is gone: an agent the code never named logs in" {
  row pip-normal agent
  mkdir -p "$T/roles/pip-normal"
  mk_token pip-normal; mkdir -p "$T/identity/pip-normal"
  run "$SCRIPT" login pip-normal
  test "$status" -eq 0
  grep -q "send-keys -t chorus-pip-normal" "$T/tmux.log"
}

# #4532 reopen 1 — an agent the runtime table names (abby-normal on agy) logs
# in and is found in her pane by the table's binary, not by a name in the code.
agy_pane_world() {
  touch "$T/no-register"
  cat > "$T/bin/tmux2" <<EOS
#!/bin/bash
case "\$1" in list-panes) echo "%9 /dev/ttys009 700"; exit 0 ;; esac
exec "$T/bin/tmux" "\$@"
EOS
  printf '#!/bin/bash\n[ "$2" = 700 ] && echo 701\nexit 0\n' > "$T/bin/pgrep"
  cat > "$T/bin/ps2" <<EOS
#!/bin/bash
if [ "\$1" = "-o" ]; then echo "\${PANE_CMD:-/h/.local/bin/agy --session run-1}"; exit 0; fi
grep -qx "\$2" "$T/alive-pids"
EOS
  chmod +x "$T/bin/tmux2" "$T/bin/pgrep" "$T/bin/ps2"
  echo 701 >> "$T/alive-pids"
  export TMUX_BIN="$T/bin/tmux2" AWAKE_PGREP="$T/bin/pgrep" AWAKE_PS="$T/bin/ps2" AWAKE_WAIT=1
  row abby-normal agent
  mkdir -p "$T/roles/abby-normal"
  mk_token abby-normal; mkdir -p "$T/identity/abby-normal"
}

@test "#4532 an agent the runtime table puts on agy logs in and is found running agy" {
  agy_pane_world
  run "$SCRIPT" login abby-normal
  test "$status" -eq 0
  grep -q "send-keys -t chorus-abby-normal" "$T/tmux.log"
  grep -q '"pid":701' "$T/sessions/abby-normal-701.json"
}

@test "#4532 NEGATIVE PROOF: a pane running claude is not taken for agy" {
  agy_pane_world
  export PANE_CMD="/h/.local/bin/claude"
  run "$SCRIPT" login abby-normal
  test "$status" -ne 0
  out_has "agy did not start"
}

@test "#4368 an agent with no role home is refused, naming where it looked" {
  row abby-normal agent
  run "$SCRIPT" login abby-normal
  test "$status" -eq 2
  out_has "abby-normal is an agent, but has no role home at"
  nothing_started
}

@test "#4368 logout reads the row too; the SessionEnd hook does not wait on it" {
  run "$SCRIPT" login wren
  row wren service
  run "$SCRIPT" logout wren
  test "$status" -eq 2
  out_has "wren is a service principal"
  printf '\n000\n' > "$T/principal-wren.json"
  run bash -c "echo '{\"reason\":\"clear\"}' | CLAUDECODE=1 CHORUS_ROLE=wren '$SCRIPT' off wren --from-exit"
  test "$status" -eq 0
}
