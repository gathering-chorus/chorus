#!/usr/bin/env bats
# @test-type: contract — runs platform/scripts/role-post against a stand-in roles door
# and fixture files in BATS_TEST_TMPDIR; no real store, no network beyond localhost.
# @domain: werk
# #4919 AC4 — at session start a role checks its own setup and refuses to work,
# naming what failed. Each check has a negative proof (#3734): the 2026-11-07
# lockout (no principal for the account), the 660 key, an unreadable identity
# dir, a live-edited settings.json, a missing Claude login, an unreachable door.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  POST="$REPO/platform/scripts/role-post"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/identity/silas" "$T/git" "$T/home/.claude"
  echo '{"k":1}' > "$T/identity/silas/nostr.json"; chmod 600 "$T/identity/silas/nostr.json"
  echo '{"model":"opus"}' > "$T/git/settings.json"; cp "$T/git/settings.json" "$T/home/.claude/settings.json"
  echo '{"oauthAccount":{"emailAddress":"x"}}' > "$T/home/.claude.json"
  echo '{"data":[{"name":"silas","hostAccount": "chorus-silas"}]}' > "$T/principals.json"
  export POST_ROLE=silas POST_ACCOUNT=chorus-silas
  export POST_IDENTITY_DIR="$T/identity" POST_HOME="$T/home" POST_SETTINGS_SOURCE="$T/git/settings.json"
  export POST_PRINCIPALS_CMD="cat $T/principals.json" POST_REACH_CMD="true" POST_STATE_DIR="$T/post" POST_LOG_CMD="$T/log" POST_NOTIFY_CMD="$T/notify"
  printf '#!/bin/sh\necho "$@" >> %s/log.out\n' "$T" > "$T/log"; chmod 755 "$T/log"
  printf '#!/bin/sh\necho "$@" >> %s/notify.out\n' "$T" > "$T/notify"; chmod 755 "$T/notify"
}

@test "control: a sound setup passes every check and says POST ok" {
  run "$POST"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "POST ok" || return 1
}

@test "negative proof: no principal for this account (the 01:18 lockout) refuses, named" {
  echo '{"data":[{"name":"silas","hostAccount": "jeffbridwell"}]}' > "$BATS_TEST_TMPDIR/principals.json"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL identity: no principal runs as account chorus-silas" || { echo "$output"; return 1; }
}

@test "negative proof: the principals door unreadable refuses, never passes" {
  export POST_PRINCIPALS_CMD="false"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL identity" || return 1
}

@test "negative proof: own key at mode 660 refuses, named" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL key: .*nostr.json mode 0660, want 0600" || { echo "$output"; return 1; }
}

@test "negative proof: identity dir not readable refuses, named" {
  export POST_IDENTITY_DIR="$BATS_TEST_TMPDIR/no-such-dir"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL identity-dir" || { echo "$output"; return 1; }
}

@test "negative proof: settings.json edited live refuses, named" {
  echo '{"model":"other"}' > "$BATS_TEST_TMPDIR/home/.claude/settings.json"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL settings: .* differs from git" || { echo "$output"; return 1; }
}

@test "negative proof: no saved Claude login refuses, named" {
  echo '{}' > "$BATS_TEST_TMPDIR/home/.claude.json"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL login" || { echo "$output"; return 1; }
}

@test "negative proof: unreachable refuses, named" {
  export POST_REACH_CMD="false"
  run "$POST"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "FAIL reach" || { echo "$output"; return 1; }
}

# ── the refusal: role-post --gate is the PreToolUse hook that holds a failed POST

gate() { printf '%s' "$1" | "$POST" --gate; }

@test "control: after a passing POST the gate lets a write through" {
  run "$POST"; [ "$status" -eq 0 ] || return 1
  run gate '{"tool_name":"Write","tool_input":{"file_path":"/x"}}'
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: after a failed POST the gate refuses Bash and Write, naming the failure" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 1 ] || return 1
  run gate '{"tool_name":"Bash","tool_input":{"command":"git status"}}'
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "POST refused" || return 1
  echo "$output" | grep -q "mode 0660, want 0600" || { echo "$output"; return 1; }
  run gate '{"tool_name":"Write","tool_input":{"file_path":"/x"}}'
  [ "$status" -eq 2 ] || return 1
}

@test "a failed POST still allows reads and re-running role-post, and a fixed POST clears the refusal" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 1 ] || return 1
  run gate '{"tool_name":"Read","tool_input":{"file_path":"/x"}}'; [ "$status" -eq 0 ] || return 1
  run gate '{"tool_name":"Bash","tool_input":{"command":"role-post"}}'; [ "$status" -eq 0 ] || return 1
  chmod 600 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 0 ] || return 1
  run gate '{"tool_name":"Bash","tool_input":{"command":"git status"}}'
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a command that only mentions role-post is still refused (role-post; rm …)" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 1 ] || return 1
  run gate '{"tool_name":"Bash","tool_input":{"command":"role-post; touch /tmp/x"}}'
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "the gate fails open on its own error: unparseable hook input passes even with a failed POST" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 1 ] || return 1
  run gate 'not json'
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a role cannot waive its own refusal (no self-waive), and the gate refuses the attempt" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 1 ] || return 1
  run gate '{"tool_name":"Bash","tool_input":{"command":"role-post --waive \"x\""}}'
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  run "$POST" --waive "the door check is wrong"
  [ "$status" -eq 2 ] || return 1
  [ -f "$BATS_TEST_TMPDIR/post/silas.failed" ] || return 1
}

@test "Jeff is told once when a role is refused, not on every re-run, and once when it clears" {
  N="$BATS_TEST_TMPDIR/notify.out"
  run "$POST"; [ "$status" -eq 0 ] || return 1
  [ ! -s "$N" ] || { echo "told Jeff on a plain pass"; cat "$N"; return 1; }
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; run "$POST"
  [ "$(grep -c '^POST refused (silas): key:' "$N")" -eq 1 ] || { cat "$N"; return 1; }
  chmod 600 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"; [ "$status" -eq 0 ] || return 1
  [ "$(grep -c '^POST ok (silas): refusal cleared' "$N")" -eq 1 ] || { cat "$N"; return 1; }
}

@test "negative proof: Jeff cannot be told is a warning, never a pass of the refusal" {
  export POST_NOTIFY_CMD=false
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST"
  [ "$status" -eq 1 ] || return 1
  echo "$output" | grep -q "WARN could not tell Jeff" || { echo "$output"; return 1; }
}

@test "session-start mode: a failed POST shows the person at the terminal a systemMessage naming the failure" {
  chmod 660 "$BATS_TEST_TMPDIR/identity/silas/nostr.json"
  run "$POST" --session-start
  [ "$status" -eq 0 ] || return 1
  echo "$output" | python3 -c 'import json,sys; d=json.load(sys.stdin); m=d["systemMessage"]; assert "POST refused" in m and "mode 0660, want 0600" in m, m' || { echo "$output"; return 1; }
}

@test "negative proof: a passing POST shows no systemMessage (nothing to alarm on)" {
  run "$POST" --session-start
  echo "$output" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "systemMessage" not in d and "POST ok" in d["hookSpecificOutput"]["additionalContext"]' || { echo "$output"; return 1; }
}
