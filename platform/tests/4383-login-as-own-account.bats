#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs (tmux, claude, token, curl, sudo); no live services, no second account
# @domain: identity
#
# #4383 — Jeff 2026-09-27: "each principal has an account (not role)". A principal
# whose Principal row names its own Mac account (hostAccount) is started as that
# account, its token copied into that account's home readable by it alone, and
# the files it writes stay group-writable (umask 002). The launcher that cannot
# start the account without a password refuses, naming the rule.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}
row() { printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$2" > "$T/principal-$1.json"; }
out_has() { printf '%s' "$output" | grep -qF -- "$1"; }

@test "#4383 a principal with its own account starts as that account" {
  row wren chorus-wren
  run "$SCRIPT" login wren
  test "$status" -eq 0
  grep -q -- "-n -u chorus-wren true" "$T/sudo.log"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "sudo -n -u chorus-wren -H bash -c"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "umask 002"
}

@test "#4383 its credentials move into that account's home, readable by it alone, and the session reads them there" {
  row wren chorus-wren
  printf '{"fixture":"wren cred"}' > "$T/identity/wren/cred.json"
  run "$SCRIPT" login wren
  d="$T/homes/chorus-wren/.chorus/identity"
  cmp -s "$d/wren/cred.json" "$T/identity/wren/cred.json"
  test "$(stat -f %Lp "$d/wren/cred.json")" = "600"
  test "$(stat -f %Lp "$d/wren")" = "700"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "CHORUS_IDENTITY_DIR='$d'"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "CHORUS_SESSIONS_DIR='$T/sessions'"
  # #4931: the stale claude.key override is retired; the start runs the self test instead
  ! grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF ".chorus/secrets/claude.key" || return 1
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "platform/scripts/role-post"
}

@test "#4383 a launcher that cannot start the account refuses, naming it, and starts nothing" {
  row wren chorus-wren
  touch "$T/sudo-deny"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  out_has "wren runs as the Mac account chorus-wren"
  out_has "/etc/sudoers.d/chorus-principals"
  test -z "$(grep -E 'respawn-pane|send-keys' "$T/tmux.log" 2>/dev/null || true)"
}

@test "#4924 a launcher without launchctl asuser permission falls back to unisolated start with a warning" {
  row wren chorus-wren
  touch "$T/launchctl-deny"
  run "$SCRIPT" login wren
  test "$status" -eq 0
  out_has "launchctl asuser"
  out_has "falling back to unisolated start"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qF "sudo -n -u chorus-wren -H bash -c"
  grep -F "respawn-pane -k -t chorus-wren" "$T/tmux.log" | grep -qv "launchctl asuser"
}

@test "#4931 NEGATIVE PROOF: an agent whose account is the launcher's own is refused, never started as it" {
  # was #4383's "starts without sudo" — the fallback Jeff ruled out 2026-11-08 11:05
  row wren "$USER"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "would run as the launcher's own account"
  test -z "$(grep -E 'respawn-pane|send-keys' "$T/tmux.log" 2>/dev/null || true)"
}

@test "#4931 NEGATIVE PROOF: an agent with no account of its own is refused, never started as the launcher" {
  row wren ""
  run "$SCRIPT" login wren
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "has no hostAccount bound"
  test -z "$(grep -E 'respawn-pane|send-keys' "$T/tmux.log" 2>/dev/null || true)"
}
