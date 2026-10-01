#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs and a local unix socket; no live services
# @domain: identity — the product domain this suite guards (#4334)
#
# #4409 — login checks the hooks daemon. It answers on a unix socket, so it is
# not one of the URL services. Down is loud, never a refusal: the role starts
# and the output says so, with the one command that restarts it.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  SOCK="/tmp/h4409-$$-$BATS_TEST_NUMBER.sock"   # short: unix socket paths cap near 104 bytes
  rm -f "$SOCK"
}
teardown() { [ -n "${LISTENER:-}" ] && kill "$LISTENER" 2>/dev/null; rm -f "$SOCK"; }

@test "#4409 hooks daemon down: the role starts anyway, and the output says so with its fix" {
  export AWAKE_HOOKS_SOCKET="$SOCK"
  run "$SCRIPT" login wren
  test "$status" -eq 0
  printf '%s' "$output" | grep -qF "hooks: the hooks daemon is not answering"
  printf '%s' "$output" | grep -qF "fix: launchctl kickstart -k gui/\$(id -u)/com.chorus.hooks"
  grep -qF "session.login.degraded wren reason=hooks-daemon-down" "$T/spine.log"
}

@test "#4409 NEGATIVE PROOF: hooks daemon answering, login says nothing about it" {
  python3 -c 'import socket,sys,time;s=socket.socket(socket.AF_UNIX);s.bind(sys.argv[1]);s.listen(8);time.sleep(60)' "$SOCK" &
  LISTENER=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -S "$SOCK" ] && break; sleep 0.2; done
  export AWAKE_HOOKS_SOCKET="$SOCK"
  run "$SCRIPT" login wren
  test "$status" -eq 0
  test "$(printf '%s' "$output" | grep -c "hooks daemon")" -eq 0
}
