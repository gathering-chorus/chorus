#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4432 — chorus-principal reads the agent roles from the roles door
# (/v1/roles/roles, roleKind agent), not from a list of three. Abby Normal is
# the fourth. When the door does not answer, the command refuses and says so;
# it never falls back to wren | silas | kade.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

@test "abby-normal is a role the command accepts" {
  run bash -c "echo '{\"session_id\":\"c-4432\",\"prompt\":\"hi\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen abby-normal"
  printf '%s' "$output" | grep -vqF "unknown role"
}

@test "a name the door does not list is refused, naming the door's roles" {
  run "$SCRIPT" relogin nobody
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "unknown role 'nobody' (abby-normal | kade | silas | wren)"
}

@test "NEGATIVE PROOF: with the door down even wren is refused, loudly" {
  touch "$T/roles-door-down"
  run bash -c "echo '{\"session_id\":\"c-4432\",\"prompt\":\"hi\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen wren"
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "REFUSED"
}
