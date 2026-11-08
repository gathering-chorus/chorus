#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs (curl records every POST/PUT, the token minter records every sign-in); no live services, no live panes.
# @domain: identity
#
# #4933 AC3 — the launch hop (Jeff's account) starts the role's process and writes
# nothing: no sign-in as the role, no session, run or presence row. The role's own
# process does all of it at its start (`chorus-principal establish <role>`), as its
# own account, through the door. Fixture role `fx` only — never a live role name.
#
# NEGATIVE PROOF (#3734): restore one launcher-side do_login/record_run call and
# the "no POST/PUT" asserts go red (shown by hand at build time, logged in the pair file).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # the fixture role: a Claude role whose account is NOT the one running this test
  mkdir -p "$T/roles/fx" "$T/identity/fx"
  printf '{"mcpServers":{"chorus-api":{"command":"chorus-runner","args":["stdio"]}}}\n' > "$T/roles/fx/.mcp.json"
  mk_token fx
  printf '{"data":{"principalKind":"agent","hostAccount":"chorus-fx"}}\n200\n' > "$T/principal-fx.json"
}

# the launch must really have happened, or "no writes" proves nothing (a refused
# start also writes nothing — the hollow pass this suite first had)
# #4933 — the pane's wrapper ran (W: the stub pane runs establish, which refuses fx
# here: its account is not the test's), so the launch reached the pane
launched() { grep -q "respawn-pane .*chorus-fx" "$T/tmux.log" && ! printf '%s' "$output" | grep -q "REFUSED"; }

writes() { grep -cE -- '-X (POST|PUT)' "$T/curl.log" 2>/dev/null || true; }
signins() { grep -c "token fx" "$T/token.log" 2>/dev/null || true; }

@test "launch: starting a stopped role signs in as nobody and writes no row" {
  : > "$T/curl.log"; : > "$T/token.log"
  run "$SCRIPT" on fx
  echo "$output"
  launched
  test "$(writes)" -eq 0
  test "$(signins)" -eq 0
  # fail closed: the wrapper's establish was refused, so the runtime never registered
  grep -q "runs as chorus-fx" "$T/establish.log"
  printf '%s' "$output" | grep -q "registered NO"
}

@test "launch: a running role with no session is not logged in by the launcher" {
  running fx 4264
  : > "$T/curl.log"; : > "$T/token.log"
  run "$SCRIPT" on fx
  echo "$output"
  # the already-awake path really ran: the role is told to establish on its next turn
  test "$status" -eq 0
  printf '%s' "$output" | grep -q "it establishes its own on its next turn"
  test "$(writes)" -eq 0
  test "$(signins)" -eq 0
}

@test "establish refuses when run by any account other than the role's own" {
  : > "$T/curl.log"; : > "$T/token.log"
  run "$SCRIPT" establish fx
  test "$status" -ne 0
  printf '%s' "$output" | grep -q "runs as chorus-fx"
  test "$(writes)" -eq 0
  test "$(signins)" -eq 0
}

@test "establish, as the role's own account, signs in as the role and writes its session through the door" {
  printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$(id -un)" > "$T/principal-fx.json"
  : > "$T/curl.log"; : > "$T/token.log"
  run "$SCRIPT" establish fx
  echo "$output"
  test "$status" -eq 0
  test "$(signins)" -ge 1
  grep -qE -- '-X POST.*/v1/identity/sessions' "$T/curl.log"
}

@test "establish after /exit goes back to the role's still-open session: one Session POST, not two (#4406)" {
  printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$(id -un)" > "$T/principal-fx.json"
  : > "$T/curl.log"
  run "$SCRIPT" establish fx
  test "$status" -eq 0
  run "$SCRIPT" establish fx
  echo "$output"
  test "$status" -eq 0
  test "$(grep -cE -- '-X POST.*/v1/identity/sessions' "$T/curl.log")" -eq 1
}

@test "RETIREMENT GATE: relogin is retired — exits 2, names its replacement, signs in as nobody" {
  : > "$T/curl.log"; : > "$T/token.log"
  run "$SCRIPT" relogin fx
  test "$status" -eq 2
  printf '%s' "$output" | grep -q "retired (#4933)"
  printf '%s' "$output" | grep -q "establishes its own session"
  test "$(writes)" -eq 0
  test "$(signins)" -eq 0
}

@test "establish --run prints ONLY the run name on stdout: the wrapper's CHORUS_RUN is that one line" {
  printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$(id -un)" > "$T/principal-fx.json"
  out=$("$SCRIPT" establish fx --run 1a2b 2>/dev/null)
  echo "stdout: [$out]"
  test "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" -eq 1
  printf '%s' "$out" | grep -qE '^[a-z-]*fx-run-1a2b$'
}
