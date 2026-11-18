#!/usr/bin/env bats
# @test-type: unit — role-post against fixture homes (no live services), and chorus-principal's
# status-line rules through its own test target
# @domain: identity
# @card: #4997
#
# #4997 — login status tells the truth. 11-09: the self-test showed every role green while
# their logins were being refused. AC1: "logged in" only with the role's own successful login
# AND an open Session row; a refused login reads refused with the reason. AC2: the self-test,
# run per role from its own account, goes red when that role's Chorus login is refused.
#
# Covers: platform/scripts/role-post
# Covers: platform/scripts/role-post-all
# Covers: platform/services/chorus-principal/src/lifecycle.rs
# Covers: directing/clearing-v2/src/views.ts

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  POST="$ROOT/platform/scripts/role-post"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/canon/roles/r/.claude" "$T/h/.claude/projects/p" "$T/idd/r" "$T/h/.chorus/identity/r"
  printf '{}' > "$T/canon/roles/r/.claude/settings.json"
  printf '{"oauthAccount": {}}' > "$T/h/.claude.json"; printf '{"refreshToken":"x"}' > "$T/h/.claude/.credentials.json"
  export CHORUS_HOME="$T/canon" POST_ACCOUNT="$(id -un)" POST_HOME="$T/h" POST_IDENTITY_DIR="$T/idd" \
    POST_STATE_DIR="$T/state" POST_NOTIFY_CMD=true POST_REACH_CMD=true \
    POST_PRINCIPALS_CMD="printf '{\"hostAccount\": \"$(id -un)\"}'" \
    POST_SETTINGS_LIVE="$T/canon/roles/r/.claude/settings.json" POST_SETTINGS_SOURCE="$T/canon/roles/r/.claude/settings.json"
}

signin() { printf '%s' "$1" > "$T/h/.chorus/identity/r/login.json"; }

@test "AC2 a recorded Chorus login reads ok in the self-test" {
  signin '{"state":"recorded","session":"r-1","pid":null,"at":"2026-11-17T20:00:00Z"}'
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "ok   signin: Chorus login recorded"
}

@test "AC2 NEGATIVE PROOF: a refused Chorus login turns the role's self-test red, naming why" {
  signin '{"state":"refused","why":"no token: invalid_client (HTTP 401)","pid":null,"at":"2026-11-17T20:00:00Z"}'
  run env POST_ROLE=r bash "$POST"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -qF "FAIL signin: Chorus login refused — no token: invalid_client (HTTP 401)"
}

@test "AC2 NEGATIVE PROOF: a pending login and a missing login file are FAIL, never a pass" {
  signin '{"state":"pending","why":"identity not answering","pid":null,"at":"x"}'
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "FAIL signin: Chorus login pending — identity not answering"
  rm -f "$T/h/.chorus/identity/r/login.json"
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "FAIL signin: unmeasured"
}

@test "AC2 the all-roles table carries the signin column" {
  grep -q 'checks="identity identity-dir key settings login signin reach"' "$ROOT/platform/scripts/role-post-all"
}

@test "AC1 the status line never says logged in without an open Session row, and says refused with why" {
  run bash -c "cd '$ROOT/platform/services/chorus-principal' && cargo test --release --quiet --test status_truth_4997 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "test result: ok\. 7 passed"
}

@test "AC1 the Teammates tile: open live row reads logged in, else not logged in, unread reads unverified" {
  run bash -c "cd '$ROOT/directing/clearing-v2' && npx jest tests/teammates-login-4997.test.ts 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "Tests: +3 passed, 3 total"
}
