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
