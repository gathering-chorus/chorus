#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal binary with the login harness stubs (tmux, claude, curl, ps); no live panes, no live claude.
# @domain: roles — the product domain this suite guards (#4334)
#
# #4845 — 2026-10-31, after Library crashed, Jeff: "when i use chorus-principal to
# log all of u in it doesnt work u all boot w no mcp access and on tmux sessions
# from 10-28". One `up` after a reboot starts every role in a fresh tmux session
# with its MCP config in place, and names any role it could not start.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # every agent role the fixture roles door lists has a home with its MCP config
  for r in wren kade silas abby-normal; do
    mkdir -p "$T/roles/$r" "$T/identity/$r"; mk_token "$r"
    printf '{"mcpServers":{"chorus-api":{"command":"chorus-runner","args":["stdio"]}}}\n' > "$T/roles/$r/.mcp.json"
  done
}

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }

@test "up starts every agent role the roles door lists, not a list of three" {
  run "$SCRIPT" up
  for r in wren kade silas abby-normal; do
    grep -q "respawn-pane -k -t chorus-$r" "$T/tmux.log" || { echo "not started: $r"; return 1; }
  done
  grep -q "/v1/roles/roles" "$T/curl.log"
}

@test "a pane left from an earlier day is replaced by a fresh session, not reused" {
  touch "$T/tmux-chorus-kade"          # chorus-kade exists, nothing live in it
  run "$SCRIPT" up
  kill_at=$(grep -n "kill-session -t chorus-kade" "$T/tmux.log" | head -1 | cut -d: -f1)
  new_at=$(grep -n "new-session -d -s chorus-kade" "$T/tmux.log" | head -1 | cut -d: -f1)
  [ -n "$kill_at" ] && [ -n "$new_at" ] && [ "$kill_at" -lt "$new_at" ]
}

@test "NEGATIVE PROOF — a live role's session is never killed" {
  running kade 5499
  run "$SCRIPT" up
  ! grep -q "kill-session -t chorus-kade" "$T/tmux.log" || return 1
}

@test "a role with no MCP config is named in the command's output and the command fails" {
  rm "$T/roles/silas/.mcp.json"
  run "$SCRIPT" up
  [ "$status" -ne 0 ]
  out_has "silas"
  out_has "MCP"
  ! grep -q "respawn-pane -k -t chorus-silas" "$T/tmux.log" || return 1
}

@test "NEGATIVE PROOF — the roles door down starts nobody and says so" {
  touch "$T/roles-door-down"
  run "$SCRIPT" up
  [ "$status" -ne 0 ]
  out_has "roles door"
  ! grep -q "respawn-pane" "$T/tmux.log" 2>/dev/null || return 1
}

# #4845 AC3 — Jeff 11-03 18:11: "do we have a single login traceid". Wren's 10-31
# login wrote about a dozen session.* events and none carried a trace.
trace_log() { printf '#!/bin/bash\necho "trace=${CHORUS_TRACE_ID:-} $*" >> "%s/spine.log"\n' "$T" > "$T/bin/chorus-log"; chmod +x "$T/bin/chorus-log"; }

@test "one login, one trace: every event it writes carries the trace it printed" {
  trace_log
  run "$SCRIPT" login silas
  [ "$status" -eq 0 ]
  t=$(printf '%s\n' "$output" | sed -n 's/^trace: //p' | head -1)
  [ -n "$t" ]
  grep -q "^trace=$t session\." "$T/spine.log"
  [ -z "$(grep -v "^trace=$t " "$T/spine.log" || true)" ] || { grep -v "^trace=$t " "$T/spine.log"; return 1; }
  # the door writes and reads of the same login carry it too (the door logs it as its trace)
  grep -q "/v1/identity/" "$T/curl.log"
  [ -z "$(grep "^curl " "$T/curl.log" | grep -vF "x-chorus-trace-id: $t" || true)" ] || { grep "^curl " "$T/curl.log" | grep -vF "x-chorus-trace-id: $t"; return 1; }
}

@test "NEGATIVE PROOF — two logins get two traces, so a fixed or empty one fails" {
  trace_log
  run "$SCRIPT" login silas; a=$(printf '%s\n' "$output" | sed -n 's/^trace: //p')
  run "$SCRIPT" logout silas
  run "$SCRIPT" login silas; b=$(printf '%s\n' "$output" | sed -n 's/^trace: //p')
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" != "$b" ]
  ! grep -q "^trace= " "$T/spine.log" || return 1
}
