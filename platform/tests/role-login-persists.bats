#!/usr/bin/env bats
# @test-type: integration — role login credential persistence and session isolation
# @domain: identity
# @card: #4924
# #4924 AC1 — A role account's Claude login saves its credentials (keychain or
# credentials file) and survives past the access-token lifetime with no re-login.
# Silas picked: start roles outside Jeff's session with launchctl asuser, so the
# keychain refuses at once and Claude writes its own file.
# Proof on the real account: chorus-silas starting this way, with no prompt and a saved login file.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PRINCIPAL_BIN="${CHORUS_PRINCIPAL_TEST_BIN:-$REPO/platform/services/chorus-principal/target/debug/chorus-principal}"
  [ -x "$PRINCIPAL_BIN" ] || PRINCIPAL_BIN="$REPO/platform/services/chorus-principal/target/release/chorus-principal"
  [ -x "$PRINCIPAL_BIN" ] || { echo "FAIL: chorus-principal not built at $PRINCIPAL_BIN" >&2; return 1; }
  T="$BATS_TEST_TMPDIR"
}

@test "AC1: chorus-principal emits Silas pinned launchctl asuser command line" {
  source "$REPO/platform/tests/lib/login-harness.bash"
  login_harness
  printf '{"data":{"principalKind":"agent","hostAccount":"chorus-silas"}}\n200\n' > "$T/principal-silas.json"

  run "$PRINCIPAL_BIN" login silas
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }

  # Shared source of truth: platform/config/chorus-role-asuser.sudoers
  # Extract the pinned rule for silas (uid 502) and match prefix against emitted command line
  rule_prefix=$(grep "chorus-silas" "$REPO/platform/config/chorus-role-asuser.sudoers" | sed -E 's/.*NOPASSWD: //; s/ \*$//')
  [ -n "$rule_prefix" ] || { echo "Missing rule in platform/config/chorus-role-asuser.sudoers"; return 1; }

  grep -F "respawn-pane -k -t chorus-silas" "$T/tmux.log" | grep -qF -- "sudo -n $rule_prefix \"" || {
    echo "tmux.log did not match repo sudoers rule ($rule_prefix):"
    cat "$T/tmux.log"
    return 1
  }
}

@test "AC1: real account chorus-silas has persistent credentials file with mode 0600" {
  role_acct="chorus-silas"
  CRED_FILE="/Users/$role_acct/.claude/.credentials.json"
  [ -f "$CRED_FILE" ] || { echo "FAIL: missing credentials file $CRED_FILE" >&2; return 1; }

  # Mode must be 0600 (-rw-------), owned by chorus-silas
  mode=$(stat -f "%OLp" "$CRED_FILE" 2>/dev/null || stat -c "%a" "$CRED_FILE")
  [ "$mode" = "600" ] || { echo "mode is $mode, expected 600"; return 1; }

  owner=$(stat -f "%Su" "$CRED_FILE" 2>/dev/null || stat -c "%U" "$CRED_FILE")
  [ "$owner" = "chorus-silas" ] || { echo "owner is $owner, expected chorus-silas"; return 1; }
}

@test "AC1: credentials survive past access-token lifetime via saved refresh token" {
  # Test with fixture matching Claude Code credentials schema
  FIXTURE="$T/credentials.json"
  cat <<'EOF' > "$FIXTURE"
{
  "claudeAiOauth": {
    "accessToken": "expired-access-token",
    "refreshToken": "valid-persistent-refresh-token",
    "expiresAt": 1000000000
  }
}
EOF
  chmod 600 "$FIXTURE"

  # Verify refresh token is present
  grep -q '"refreshToken": *"valid-persistent-refresh-token"' "$FIXTURE" || return 1

  # Check that role-post recognizes valid credentials
  export POST_ROLE=silas POST_ACCOUNT=chorus-silas
  export POST_HOME="$T"
  mkdir -p "$T/.claude"
  cp "$FIXTURE" "$T/.claude/.credentials.json"
  echo '{"oauthAccount":{"emailAddress":"silas@chorus.local"}}' > "$T/.claude.json"

  run "$REPO/platform/scripts/role-post" silas
  # The login check must pass as valid
  echo "$output" | grep -q "ok   login: Claude login valid" || { echo "$output"; return 1; }
}

@test "negative proof: login session inheriting Jeff's login keychain is detected and rejected" {
  # When running in a session that inherits Jeff's login keychain, a role login must refuse
  # rather than pop up GUI prompts on Jeff's screen.
  jeff_user="jeffbridwell"
  run env TEST_MOCK_KEYCHAIN="/Users/$jeff_user/Library/Keychains/login.keychain-db" \
    "$REPO/platform/scripts/role-keychain-check" chorus-silas
  [ "$status" -eq 1 ] || { echo "expected refusal on Jeff's keychain, got status $status: $output"; return 1; }
  echo "$output" | grep -q "REFUSED: session inherits caller login keychain" || return 1
}

@test "AC1 control: sudoers allows pinned launchctl asuser for chorus-silas" {
  if [ "$(id -un)" != "jeffbridwell" ]; then skip "requires runner jeffbridwell"; fi
  run sudo -n -l /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c true
  [ "$status" -eq 0 ] || { echo "control failed: sudoers rule missing or broken: $output" >&2; return 1; }
  [[ "$output" == *"NOPASSWD"* ]] || { echo "control failed: rule not NOPASSWD: $output" >&2; return 1; }
}

@test "AC1 negative proof: sudoers refuses launchctl asuser for root (uid 0)" {
  if [ "$(id -un)" != "jeffbridwell" ]; then skip "requires runner jeffbridwell"; fi
  # The control case keeps the negative honest: control must pass first
  sudo -n -l /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c true >/dev/null 2>&1 || { echo "control failed: sudoers not active" >&2; return 1; }
  run sudo -n /bin/launchctl asuser 0 /bin/sh -c id
  [ "$status" -ne 0 ]
  [[ "$output" == *"password is required"* || "$output" == *"not allowed"* ]] || { echo "refusal output unexpected: $output" >&2; return 1; }
}

@test "AC1 negative proof: sudoers refuses unpinned command for uid 502" {
  if [ "$(id -un)" != "jeffbridwell" ]; then skip "requires runner jeffbridwell"; fi
  # The control case keeps the negative honest: control must pass first
  sudo -n -l /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c true >/dev/null 2>&1 || { echo "control failed: sudoers not active" >&2; return 1; }
  run sudo -n /bin/launchctl asuser 502 /bin/sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"password is required"* || "$output" == *"not allowed"* ]] || { echo "refusal output unexpected: $output" >&2; return 1; }
}

@test "AC1 negative proof: sudoers refuses uid / account mismatch (uid 502 with chorus-wren)" {
  if [ "$(id -un)" != "jeffbridwell" ]; then skip "requires runner jeffbridwell"; fi
  # The control case keeps the negative honest: control must pass first
  sudo -n -l /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c true >/dev/null 2>&1 || { echo "control failed: sudoers not active" >&2; return 1; }
  run sudo -n /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-wren -H bash -c true
  [ "$status" -ne 0 ]
  [[ "$output" == *"password is required"* || "$output" == *"not allowed"* ]] || { echo "refusal output unexpected: $output" >&2; return 1; }
}

@test "demo-4924-ac1.sh supports dry-run without executing live start" {
  [ -x "$REPO/platform/scripts/demo-4924-ac1.sh" ]
  run bash "$REPO/platform/scripts/demo-4924-ac1.sh" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"dry-run ok"* ]] || return 1
}

@test "NEGATIVE PROOF: demo-4924-ac1.sh refuses invalid arguments" {
  run bash "$REPO/platform/scripts/demo-4924-ac1.sh" --bogus
  [ "$status" -ne 0 ]
}
