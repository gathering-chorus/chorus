#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4925 round 5 (Jeff 11-23 08:10 "wait for silas to fix it") — Jeff's `logout <role>`
# closes that role's session and stops its pane. Measured 11-22 15:55: every logout he
# ran read "session":"" because the role's login lives in its own account's home (0700),
# not his; nothing closed and the role's next start resumed its 11-09 row. The door lets
# only the row's owner end it, so the role's own account closes its own login.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/target/$(id -un)/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  est wren
  "$SCRIPT" login wren >/dev/null 2>&1
  OLD=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["session"])' "$T/identity/wren/login.json")
  # the login as it really sits: in the role account's home, unreadable from the caller's
  OWN="$T/homes/$(id -un)/.chorus/identity/wren"
  mkdir -p "$OWN" && mv "$T/identity/wren/login.json" "$OWN/login.json"
}
closes() { ls "$T"/bodies/*-PATCH-identity_sessions_"$OLD".json 2>/dev/null | xargs cat 2>/dev/null | grep -c '"sessionState":"closed"' || true; }

@test "#4925 NEGATIVE: logout of a role whose login is in its own home closes that login, through its own account" {
  run "$SCRIPT" logout wren
  test "$status" -eq 0
  grep -q -- "-u $(id -un) -H env CHORUS_IDENTITY_DIR=$T/homes/$(id -un)/.chorus/identity .* close-own wren" "$T/sudo.log"
  test "$(closes)" -eq 1
  echo "$output" | grep -qF "wren logged out: login closed (session $OLD)"
  grep -q '"state":"closed"' "$OWN/login.json"
}

# not a negative proof of anything new (the kill existed before); kept so the round's AC reads whole
@test "#4925 the pane is stopped by the caller's logout" {
  running wren 4925
  run "$SCRIPT" logout wren
  test "$status" -eq 0
  grep -q "kill-session -t chorus-wren" "$T/tmux.log"
}

@test "#4925 NEGATIVE: the door refuses the role's own logout, and the logout says the login stays open" {
  echo 403 > "$T/curl.status.identity_logouts"
  run "$SCRIPT" logout wren
  test "$(closes)" -eq 0
  echo "$output" | grep -qF "the login row $OLD could not be closed"
  if echo "$output" | grep -qF "login closed (session"; then false; fi
  grep -q '"state":"recorded"' "$OWN/login.json"
}

# #4925 round 5 (Jeff 09:13 via Wren) — a new run's Presence starts unknown; the run asks
# for its first delivery, so the Presence turns reachable without a test's own nudge
firsts() { [ -f "$T/nudge.log" ] || { echo 0; return; }; grep -c "^nudge-send wren wren \[presence\] new run" "$T/nudge.log" || true; }
turn4925() { echo '{"session_id":"c-4925","prompt":"hello"}' | AWAKE_SEEN_SYNC=1 AWAKE_SEEN_EVERY=0 "$SCRIPT" seen wren >/dev/null 2>&1; }

@test "#4925 NEGATIVE: a new run sends the role one first message, and a live run sends none" {
  # setup's login recorded wren's run: that new run asked for its first delivery, once
  test "$(firsts)" -eq 1
  mv "$OWN/login.json" "$T/identity/wren/login.json"   # this case runs as the role: its login is its own
  # turns on the live run record no new run, so they send nothing more
  turn4925
  turn4925
  test "$(firsts)" -eq 1
}
