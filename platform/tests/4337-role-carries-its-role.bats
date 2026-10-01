#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake and chorus-principal binaries with stub tmux, claude, ps, token-minter, curl, service probe, osascript and open; no live services, no live panes.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4337 — Jeff 2026-09-26: "i dont want to have to disassemble and reassemble the
# car evertime i want to turn an agent on or off"; "i dont want the 10 steps i
# need to run when the 1 step command fails". That morning Kade's session ran as
# Wren: `claude attach` put it inside the Claude daemon's warm spare, started from
# Wren's pane with her env. The fix takes the layer away: roles run with no
# daemon (disableAgentView), `on` resumes a background conversation in its own
# pane instead of attaching, `off` ends the background copy, and a process that
# carries another role is never called logged in.
setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  PRINCIPAL="$SCRIPT"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  # #4409 — the shared fixture world (lib/login-harness.bash), not a copy of it
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }
state_is() { grep -q "\"state\":\"$2\"" "$T/identity/$1/login.json"; }

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }
state_is() { grep -q "\"state\":\"$2\"" "$T/identity/$1/login.json"; }

@test "the launch turns the daemon off and names the role, and nothing is attached" {
  run "$SCRIPT" on kade
  test "$status" -eq 0
  grep -qF "CLAUDE_CODE_DISABLE_AGENT_VIEW=1" "$T/tmux.log"
  grep -qF "CHORUS_ROLE='kade'" "$T/tmux.log"
  test -z "$(grep -F "claude attach" "$T/tmux.log" || true)"
}

@test "every role's settings turn Claude's background daemon off" {
  for r in wren kade silas; do
    python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("disableAgentView") is True else 1)' "$ROOT/roles/$r/.claude/settings.json"
  done
}

@test "off ends the role's background copy of the conversation" {
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
[ "\$1" = "agents" ] && echo '[{"id":"bda5f062","kind":"background","sessionId":"bda5f062-da2b"},{"id":"x1","kind":"interactive"}]'
exit 0
EOS
  chmod +x "$T/bin/claude"
  running kade 4264
  run "$SCRIPT" off kade
  test "$status" -eq 0
  grep -qx "claude stop bda5f062" "$T/claude.log"
  test -z "$(grep -x "claude stop x1" "$T/claude.log" || true)"
}

wrong_env() {  # ps answering `eww`: CHORUS_ROLE=$1 on the first look, $2 (default: $1) after
  printf '%s\n%s\n' "$1" "${2:-$1}" > "$T/env-seq"
  cat > "$T/bin/ps3" <<EOS
#!/bin/bash
if [ "\$1" = "eww" ]; then r=\$(head -1 "$T/env-seq"); [ \$(wc -l < "$T/env-seq") -gt 1 ] && sed -i '' 1d "$T/env-seq"; echo "/h/.local/bin/claude -c PWD=/x CHORUS_ROLE=\$r TERM=xterm"; exit 0; fi
grep -qx "\$2" "$T/alive-pids"
EOS
  chmod +x "$T/bin/ps3"; export AWAKE_PS="$T/bin/ps3"
}

@test "a process carrying another role is repaired by on itself: that pane ends and the role starts again, logged in" {
  wrong_env wren kade
  run "$SCRIPT" on kade
  test "$status" -eq 0
  out_has "runs as wren, not kade — ending that pane and starting kade again"
  grep -q "kill-session -t chorus-kade" "$T/tmux.log"
  out_has "logged in"
  grep -q "session.wrong_role kade" "$T/spine.log"
}

@test "still wrong after one restart: refused as WRONG ROLE with ONE next command, never 'logged in'" {
  wrong_env wren wren
  run "$SCRIPT" on kade
  test "$status" -ne 0
  out_has "WRONG ROLE"
  out_has "Next: chorus-principal login kade"
  out_lacks "&&"
  out_lacks "logged in  via"
}

@test "NEGATIVE PROOF: a process that carries its own role is neither restarted nor refused" {
  wrong_env kade
  run "$SCRIPT" on kade
  test "$status" -eq 0
  out_has "logged in"
  out_lacks "WRONG ROLE"
  test -z "$(grep -F "kill-session" "$T/tmux.log" || true)"
}
