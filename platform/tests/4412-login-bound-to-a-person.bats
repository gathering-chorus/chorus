#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4412 — Jeff 2026-09-30: "maybe we must bind chorus-principal to a human(s)" /
# "the login and logout". A role's session starts and ends only while a person
# is signed in (an open browser Session: his Clearing sign-in), and the row says
# who: startedBy / endedBy. /exit is not gated: it ends only the run (#4406).
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}
posts() { ls "$T/bodies" 2>/dev/null | grep -c -- '-POST-identity_sessions.json$' || true; }

@test "#4412 login with a person signed in records who started it" {
  run "$SCRIPT" login wren
  test "$status" -eq 0
  has "$(cat "$T"/bodies/*-POST-identity_sessions.json | tail -1)" '"startedBy":"jeff"'
}

@test "#4412 NEGATIVE: login with nobody signed in is refused and writes nothing" {
  touch "$T/nobody-signed-in"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "nobody is signed in"
  printf '%s' "$output" | grep -qF "fix: sign in at https://clearing.lightlifeurbangardens.com"
  test "$(posts)" -eq 0
}

@test "#4412 NEGATIVE: chorus-api not answering is refused with the one command that fixes it" {
  touch "$T/api-down"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "fix: agent-state.sh restart chorus-api"
  test "$(posts)" -eq 0
}

@test "#4412 logout with a person signed in records who ended it" {
  "$SCRIPT" login wren >/dev/null 2>&1
  run "$SCRIPT" logout wren
  has "$(cat "$T"/bodies/*PUT-identity_sessions_* | tail -1)" '"endedBy":"jeff"'
}

@test "#4412 NEGATIVE: logout with nobody signed in is refused and closes nothing" {
  "$SCRIPT" login wren >/dev/null 2>&1
  touch "$T/nobody-signed-in"
  run "$SCRIPT" logout wren
  test "$status" -eq 2
  test "$(cat "$T"/bodies/*PUT-identity_sessions_* 2>/dev/null | grep -c '"sessionState":"closed"' || true)" -eq 0
}

@test "#4412 /exit is not gated on a person (it ends only the run)" {
  "$SCRIPT" login wren >/dev/null 2>&1
  touch "$T/nobody-signed-in"
  run bash -c "echo '{\"reason\":\"prompt_input_exit\"}' | CLAUDECODE=1 CHORUS_ROLE=wren '$SCRIPT' off wren --from-exit"
  test "$status" -eq 0
}
