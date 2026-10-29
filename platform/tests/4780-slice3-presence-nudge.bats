#!/usr/bin/env bats
# @test-type: integration — drives chorus-principal with login-harness stub world
# @domain: identity — guards #4786 (Slice 3 of #4780): Presence hostAccount & cleanup
#
# AC3: Presence records real hostAccount, retires on run end, and cleans dangling deliveredTo.
# Negative proof (#3734): a fixture where launcher USER is passed does not contaminate
# hostAccount, and presence after run end is marked unreachable.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }
out_lacks() { test -z "$(printf '%s' "$output" | grep -F -- "$1" || true)"; }

@test "AC3: login records role account in hostAccount, not launcher USER" {
  export USER="jeffbridwell"
  run "$SCRIPT" on wren
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-wren"'
  test -z "$(printf '%s' "$s" | grep -F '"hostAccount":"jeffbridwell"' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-wren"'
  test -z "$(printf '%s' "$p" | grep -F '"hostAccount":"jeffbridwell"' || true)"
}

@test "NEGATIVE PROOF: launcher USER is rejected from contaminating hostAccount" {
  export USER="attacker-or-launcher"
  run "$SCRIPT" on silas
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$s" | grep -F 'attacker-or-launcher' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$p" | grep -F 'attacker-or-launcher' || true)"
}

@test "AC3: presence retires to unreachable on run end and logs spine event" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren
  test "$status" -eq 0

  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1)
  has "$p" '"reachability":"unreachable"'

  grep -q "session.presence.retired wren" "$T/spine.log"
}

@test "NEGATIVE PROOF: a retired presence does not stay reachable or unknown" {
  run "$SCRIPT" on wren
  run "$SCRIPT" off wren

  p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1)
  test -z "$(printf '%s' "$p" | grep -F '"reachability":"reachable"' || true)"
  test -z "$(printf '%s' "$p" | grep -F '"reachability":"unknown"' || true)"
}

@test "NEGATIVE PROOF: CHORUS_ROLE_ACCOUNT=chorus-kade for silas -> still chorus-silas" {
  export CHORUS_ROLE_ACCOUNT="chorus-kade"
  run "$SCRIPT" on silas
  test "$status" -eq 0

  s=$(body POST identity_sessions)
  has "$s" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$s" | grep -F '"hostAccount":"chorus-kade"' || true)"

  p=$(body POST identity_presences)
  has "$p" '"hostAccount":"chorus-silas"'
  test -z "$(printf '%s' "$p" | grep -F '"hostAccount":"chorus-kade"' || true)"
}

@test "NEGATIVE PROOF: principal with missing hostAccount in identity door is refused loudly" {
  printf '{"data":{"principalKind":"agent"}}\n200\n' > "$T/principal-silas.json"
  run "$SCRIPT" on silas
  test "$status" -ne 0
  out_has "REFUSED"
  out_has "principal silas has no hostAccount bound in the identity door"
}
