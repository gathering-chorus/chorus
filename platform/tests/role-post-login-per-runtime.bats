#!/usr/bin/env bats
# @test-type: unit — role-post against fixture homes; no live services (identity and reach stubbed)
# @domain: identity
#
# #4931 AC2 — the self test judges each role's login by its own runtime, read as the
# role. Measured 2026-11-08 08:17: it judged Abby and Borg Queen (agy) by a Claude login
# and failed them, and from another account it could not read Claude logins at all.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  POST="$ROOT/platform/scripts/role-post"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/canon/roles/r/.claude" "$T/canon/roles/g/.gemini" "$T/h/.claude/projects/p" "$T/idd/r" "$T/idd/g"
  printf '{}' > "$T/canon/roles/r/.claude/settings.json"; printf '{}' > "$T/canon/roles/g/.gemini/settings.json"
  export CHORUS_HOME="$T/canon" POST_ACCOUNT="$(id -un)" POST_HOME="$T/h" POST_IDENTITY_DIR="$T/idd" \
    POST_STATE_DIR="$T/state" POST_NOTIFY_CMD=true POST_REACH_CMD=true \
    POST_PRINCIPALS_CMD="printf '{\"hostAccount\": \"$(id -un)\"}'" \
    POST_SETTINGS_LIVE="$T/canon/roles/r/.claude/settings.json" POST_SETTINGS_SOURCE="$T/canon/roles/r/.claude/settings.json"
}
claude_login() { printf '{"oauthAccount": {}}' > "$T/h/.claude.json"; printf '{"refreshToken":"x"}' > "$T/h/.claude/.credentials.json"; }

@test "#4931 a Claude login saved and never refused reads ok" {
  claude_login
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "ok   login: Claude login valid"
}

@test "#4931 NEGATIVE PROOF: a Claude 401 after the login was saved reads FAIL" {
  claude_login; sleep 1
  printf '{"apiErrorStatus":401,"timestamp":"%s"}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$T/h/.claude/projects/p/s.jsonl"
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "FAIL login: Claude refused it after it was saved"
}

@test "#4931 a conversation that only mentions a 401 is not a refusal" {
  claude_login; sleep 1
  printf '{"message":"we saw OAuth token revoked earlier","timestamp":"%s"}\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$T/h/.claude/projects/p/s.jsonl"
  run env POST_ROLE=r bash "$POST"
  printf '%s\n' "$output" | grep -qF "ok   login: Claude login valid"
}

@test "#4931 a Gemini (agy) role is judged by its own sign-in and its .gemini settings" {
  mkdir -p "$T/h/.gemini/antigravity-cli"; printf 'tok' > "$T/h/.gemini/antigravity-cli/antigravity-oauth-token"
  run env -u POST_SETTINGS_LIVE -u POST_SETTINGS_SOURCE POST_ROLE=g POST_SETTINGS_LIVE="$T/canon/roles/g/.gemini/settings.json" POST_SETTINGS_SOURCE="$T/canon/roles/g/.gemini/settings.json" bash "$POST"
  printf '%s\n' "$output" | grep -qF "ok   login: Gemini (agy) sign-in present"
  ! printf '%s\n' "$output" | grep -q "Claude login" || return 1
}

@test "#4931 NEGATIVE PROOF: a Gemini role with no sign-in reads FAIL" {
  run env POST_ROLE=g POST_SETTINGS_LIVE="$T/canon/roles/g/.gemini/settings.json" POST_SETTINGS_SOURCE="$T/canon/roles/g/.gemini/settings.json" bash "$POST"
  printf '%s\n' "$output" | grep -qF "FAIL login: no Gemini (agy) sign-in"
}

@test "#4931 every run writes a result the five-role table can read, with no secret in it" {
  claude_login
  run env POST_ROLE=r bash "$POST"
  test -r "$T/state/r.result"
  head -1 "$T/state/r.result" | grep -q "^at [0-9]* role r account"
  ! grep -q "refreshToken\|accessToken" "$T/state/r.result" || return 1
}
