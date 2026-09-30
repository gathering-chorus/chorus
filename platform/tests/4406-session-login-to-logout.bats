#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4406 — a session is login to logout (Jeff 2026-09-30: "session starts when i
# do chorus-principal login and ends when i do logout"). No timer ends it: the
# #4384 24h lifetime is gone. /exit ends the Claude run, not the session, and
# the next login goes back to the same open session with a new run.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}
turn() { run bash -c "echo '{\"session_id\":\"c-4384\",\"prompt\":\"<task-notification>x</task-notification>\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen wren"; }
row() { python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$T/identity/wren/session.row.json" "$1"; }
age() {  # started $1 hours ago
  python3 - "$T/identity/wren/session.row.json" "$1" <<'P'
import json,sys,time
p,h=sys.argv[1],float(sys.argv[2]); v=json.load(open(p))
v["startedAt"]=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime(time.time()-h*3600)); json.dump(v,open(p,"w"))
P
}
posts() { ls "$T/bodies" | grep -c -- '-POST-identity_sessions.json$' || true; }

@test "#4406 a session is never ended by its age" {
  "$SCRIPT" login wren >/dev/null 2>&1
  old=$(row name)
  age 200
  turn
  test "$status" -eq 0
  test "$(grep -c "session.expired" "$T/spine.log" || true)" -eq 0
  test "$(posts)" -eq 1
  test "$(row name)" = "$old"
}

@test "#4406 /exit ends the run, not the session" {
  "$SCRIPT" login wren >/dev/null 2>&1
  export AWAKE_SEEN_EVERY=0; turn
  run bash -c "echo '{\"reason\":\"prompt_input_exit\"}' | CLAUDECODE=1 CHORUS_ROLE=wren '$SCRIPT' off wren --from-exit"
  test "$status" -eq 0
  has "$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1)" '"endReason":"exit"'
  grep -q '"state":"recorded"' "$T/identity/wren/login.json"
  test "$(cat "$T"/bodies/*PUT-identity_sessions_* 2>/dev/null | grep -c '"sessionState":"closed"' || true)" -eq 0
}

@test "#4406 a login after /exit goes back to the same session" {
  "$SCRIPT" login wren >/dev/null 2>&1
  old=$(row name)
  run bash -c "echo '{\"reason\":\"prompt_input_exit\"}' | CLAUDECODE=1 CHORUS_ROLE=wren '$SCRIPT' off wren --from-exit"
  "$SCRIPT" login wren >/dev/null 2>&1
  test "$(posts)" -eq 1
  test "$(row name)" = "$old"
  grep -q "session.resumed wren" "$T/spine.log"
}

@test "#4406 NEGATIVE: logout ends the session, and the next login starts a new one" {
  "$SCRIPT" login wren >/dev/null 2>&1
  old=$(row name)
  "$SCRIPT" logout wren >/dev/null 2>&1
  has "$(cat "$T"/bodies/*PUT-identity_sessions_* | tail -1)" '"sessionState":"closed"'
  "$SCRIPT" login wren >/dev/null 2>&1
  test "$(posts)" -eq 2
  test "$(row name)" != "$old"
}
