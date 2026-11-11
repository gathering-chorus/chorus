#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4385 — revocation bites mid-session: a logged-in role whose principal is
# revoked (row gone, or no longer an agent) has its next turn refused, the run
# ended as revoked and the session closed, said once; later turns are refused
# in one line. An identity API that does not answer revokes nothing. Logout
# clears the pane and ends with "logged out", so old text cannot read as the
# result (09-28 05:23).
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # #4933 — each role's own wrapper establishes its session (as the account running
  # the test); the launcher writes nothing, so every row below is the role's own
  for r in silas kade wren; do est "$r"; done
}
turn() { run bash -c "echo '{\"session_id\":\"c-4385\",\"prompt\":\"<task-notification>x</task-notification>\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen wren"; }
revoke() { printf '{}\n404\n' > "$T/principal-wren.json"; }
out_has() { printf '%s' "$output" | grep -qF -- "$1"; }

@test "#4385 a revoked principal's next turn is refused, its run ends as revoked, its session closes" {
  "$SCRIPT" login wren >/dev/null 2>&1
  revoke
  turn
  test "$status" -eq 2
  out_has "wren's principal is revoked"
  has "$(cat "$T"/bodies/*PATCH-identity_sessionruns_* | tail -1)" '"endReason":"revoked"'
  has "$(cat "$T"/bodies/*PATCH-identity_sessions_* | tail -1)" '"sessionState":"closed"'
  grep -q "session.revoked wren" "$T/spine.log"
}

@test "#4385 later turns are refused in one line and write nothing" {
  "$SCRIPT" login wren >/dev/null 2>&1
  revoke; turn
  n=$(ls "$T/bodies" | wc -l)
  turn
  test "$status" -eq 2
  out_has "wren's session was ended: its principal is revoked"
  test "$(ls "$T/bodies" | wc -l)" -eq "$n"
  test "$(grep -c "session.revoked wren" "$T/spine.log")" -eq 1
}

@test "#4895 a revoked role's turn END exits 0: on a Stop hook, exit 2 would mean keep going" {
  "$SCRIPT" login wren >/dev/null 2>&1
  revoke; turn                      # the prompt path ends it as revoked (exit 2, above)
  run bash -c "echo '{\"hook_event_name\":\"Stop\",\"session_id\":\"c-4385\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen wren"
  test "$status" -eq 0
  # NEGATIVE PROOF: the same revoked role's prompt is still refused (the turn-end path is the only exemption)
  turn
  test "$status" -eq 2
}

@test "#4385 NEGATIVE PROOF: an unrevoked principal's turn runs untouched" {
  "$SCRIPT" login wren >/dev/null 2>&1
  turn
  test "$status" -eq 0
  test -z "$(cat "$T"/bodies/*PATCH-identity_sessionruns_* 2>/dev/null | grep -F revoked || true)"
}

@test "#4385 NEGATIVE PROOF: an identity API that does not answer revokes nothing" {
  "$SCRIPT" login wren >/dev/null 2>&1
  printf '\n000\n' > "$T/principal-wren.json"
  turn
  test "$status" -eq 0
  test -z "$(grep -F "session.revoked" "$T/spine.log" || true)"
}

@test "#4385 a principal that is restored can log in again and its turns run" {
  "$SCRIPT" login wren >/dev/null 2>&1
  revoke; turn
  est wren      # restored: the row answers agent again, naming wren's account (#4933: stated, not the default)
  run "$SCRIPT" login wren
  test "$status" -eq 0
  turn
  test "$status" -eq 0
}

@test "#4385 logout's last line says logged out" {
  "$SCRIPT" login wren >/dev/null 2>&1
  run "$SCRIPT" logout wren
  test "$status" -eq 0
  printf '%s\n' "$output" | tail -1 | grep -q "wren logged out"
}
