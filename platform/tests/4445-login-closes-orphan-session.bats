#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal binary in the shared login harness (stub curl answers the session listing); never reaches the identity API.
# @domain: identity — the product domain this suite guards (#4334)
#
# #4445 — Jeff, 2026-10-08 08:03: "i now see 2 sessions for abby" / "i used our
# chorus-principal commands 2x". The first `on` wrote its Session row and was
# interrupted before it recorded it, so nothing ever closed that row. A login
# now closes every other open session of the same role on the same channel: a
# role has one session, login to logout (#4406). Rows are shaped like the real
# listing (actsAs "role-<name>"): the first fixture used a bare name and passed
# while prod (10:02, 10:04) never matched.
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

# the session listing the API answers: silas's orphan, a closed silas row, kade's
# open row, and a silas agent-channel row — only the first is silas's stray pane session
listing() {
  printf '{"data":[%s]}\n' "$1" > "$T/existing.json"
}
ROW() { printf '{"name":"%s","actsAs":"role-%s","ownedBy":"principal-%s","tokenId":"jti-%s","channel":"%s","sessionState":"%s","startedAt":"2026-10-08T10:41:18Z"}' "$1" "$2" "$2" "$1" "$3" "$4"; }

@test "a login closes the open session an interrupted login left behind" {
  listing "$(ROW session-silas-orphan silas pane open),$(ROW session-silas-old silas pane closed),$(ROW session-kade-live kade pane open),$(ROW session-silas-agent silas agent open)"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  ls "$T/bodies" | grep -q 'PUT-identity_sessions_session-silas-orphan'
  grep -q '"sessionState":"closed"' "$T"/bodies/*-PUT-identity_sessions_session-silas-orphan.json
  printf '%s' "$output" | grep -qF "closed 1 older open session"
  grep -q 'session.superseded silas' "$T/spine.log"
}

@test "NEGATIVE PROOF: another role's session, a closed row and another channel are never closed" {
  listing "$(ROW session-silas-old silas pane closed),$(ROW session-kade-live kade pane open),$(ROW session-silas-agent silas agent open)"
  run "$SCRIPT" on silas
  test "$status" -eq 0
  test -z "$(ls "$T/bodies" | grep PUT-identity_sessions || true)"
  test "$(grep -c 'session.superseded' "$T/spine.log" || true)" -eq 0
}
