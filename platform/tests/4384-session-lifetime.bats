#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4384 — a session has an absolute lifetime. Renewal keeps a token alive, so
# before this a login never ended. A session older than CHORUS_SESSION_MAX_HOURS
# (default 24) is closed on its next turn and the role is logged in again on
# its own: a new Session row, the turn runs, Jeff types nothing. A login that
# is refused then refuses the turn and says to log in again.
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

@test "#4384 a session past its lifetime is closed on the next turn and the role is logged in again" {
  "$SCRIPT" login wren >/dev/null 2>&1
  old=$(row name)
  age 25
  turn
  test "$status" -eq 0
  grep -q "session.expired wren" "$T/spine.log"
  has "$(cat "$T/bodies/"*"PUT-identity_sessions_${old}"* | tail -1)" '"sessionState":"closed"'
  test "$(posts)" -eq 2
  test "$(row name)" != "$old"
}

@test "#4402 a lifetime re-login ends the old run, and the next turn records a run of the new session" {
  "$SCRIPT" login wren >/dev/null 2>&1
  turn                                          # the first turn records this login's run
  oldrun=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$T/identity/wren/run.row.json")
  age 25
  turn                                          # crosses the cap: old session closed, new login
  has "$(cat "$T/bodies/"*"PUT-identity_sessionruns_${oldrun}"* | tail -1)" '"endReason":"restart"'
  export AWAKE_SEEN_EVERY=0                     # writes are throttled to one a minute; this turn is due
  turn                                          # the next turn records the new session's run
  new=$(row name)
  test "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("runOf",""))' "$T/identity/wren/run.row.json")" = "$new"
}

@test "#4403 a seen-write racing the 24h re-login never records a run of the old session" {
  "$SCRIPT" login wren >/dev/null 2>&1
  turn                                          # this login's run
  old=$(row name)
  before=$(ls "$T"/bodies/*-POST-identity_sessionruns.json 2>/dev/null | wc -l | tr -d ' ')
  age 25
  echo 2 > "$T/token-slow"                      # every token call takes 2s, so the re-login's steps are 2s apart
  ( echo '{"session_id":"c-4403","prompt":"<task-notification>x</task-notification>"}' | AWAKE_SEEN_SYNC=1 "$SCRIPT" seen wren >/dev/null 2>&1 ) &
  sleep 3                                       # mid re-login: one step done, the next under way
  AWAKE_SEEN_EVERY=0 "$SCRIPT" seen-write wren "" delivered >/dev/null 2>&1   # what a surfaced nudge starts
  wait
  rm -f "$T/token-slow"
  stale=0
  for b in $(ls "$T"/bodies/*-POST-identity_sessionruns.json | tail -n +$((before+1))); do
    stale=$((stale + $(grep -cF "\"runOf\":\"$old\"" "$b" || true)))
  done
  test "$stale" -eq 0
}

@test "#4384 NEGATIVE: a session under its lifetime is left alone" {
  "$SCRIPT" login wren >/dev/null 2>&1
  old=$(row name)
  age 23
  turn
  test "$status" -eq 0
  test "$(grep -c "session.expired" "$T/spine.log" || true)" -eq 0
  test "$(posts)" -eq 1
  test "$(row name)" = "$old"
}

@test "#4384 the lifetime is CHORUS_SESSION_MAX_HOURS" {
  "$SCRIPT" login wren >/dev/null 2>&1
  age 3
  export CHORUS_SESSION_MAX_HOURS=2
  turn
  grep -q "session.expired wren" "$T/spine.log"
}

@test "#4384 a login refused at the lifetime refuses the turn and says to log in again" {
  "$SCRIPT" login wren >/dev/null 2>&1
  age 25
  export AWAKE_REFUSE_ON_LOGIN_FAILURE=1
  touch "$T/token-fail"
  turn
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "log in again"
}
