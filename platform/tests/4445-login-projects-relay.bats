#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal binary in the shared login harness (stub relay projection, stub tmux/claude/curl); never reaches the relay.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4445 — Abby published 48 replies on 2026-10-07 and none reached the Clearing:
# her nostr key was in the graph but not on the relay's member list, because
# nothing ever ran buzz-allowlist-project. Wren: "why didn't her login trigger
# it?" Login now runs the projection, so a principal whose key is in the graph
# is a relay member by the time its session starts.
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

out_has() { printf '%s' "$output" | grep -qF -- "$1"; }
state_is() { grep -q "\"state\":\"$2\"" "$T/identity/$1/login.json"; }

@test "a login runs the relay projection and prints what it did" {
  run "$SCRIPT" on silas
  test "$status" -eq 0
  test "$(grep -c '^relay' "$T/relay.log")" -eq 1
  out_has "relay: projected: graph=4 keys, +1 -0"
  grep -q 'session.relay.projected silas' "$T/spine.log"
}

@test "a failing projection never blocks the login, and names the fix" {
  touch "$T/relay-fail"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  state_is silas recorded
  out_has "relay: the relay member list was not updated"
  out_has "Operation timed out"
  out_has "fix: bash $T/bin/relay"
  grep -q 'session.relay.failed silas' "$T/spine.log"
}

@test "NEGATIVE PROOF: with the projection switched off, nothing reaches the relay stub" {
  export AWAKE_RELAY_PROJECT=none
  run "$SCRIPT" on silas
  test "$status" -eq 0
  test ! -e "$T/relay.log"
}
