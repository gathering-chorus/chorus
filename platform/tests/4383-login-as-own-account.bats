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
  grep -F "send-keys -t chorus-wren" "$T/tmux.log" | grep -qF "sudo -n -u chorus-wren -H bash -c"
  grep -F "send-keys -t chorus-wren" "$T/tmux.log" | grep -qF "umask 002"
}

@test "#4383 its credentials move into that account's home, readable by it alone, and the session reads them there" {
  row wren chorus-wren
  printf '{"fixture":"wren cred"}' > "$T/identity/wren/cred.json"
  run "$SCRIPT" login wren
  d="$T/homes/chorus-wren/.chorus/identity"
  cmp -s "$d/wren/cred.json" "$T/identity/wren/cred.json"
  test "$(stat -f %Lp "$d/wren/cred.json")" = "600"
  test "$(stat -f %Lp "$d/wren")" = "700"
  grep -F "send-keys -t chorus-wren" "$T/tmux.log" | grep -qF "CHORUS_IDENTITY_DIR='$d'"
}

@test "#4383 a launcher that cannot start the account refuses, naming it, and starts nothing" {
  row wren chorus-wren
  touch "$T/sudo-deny"
  run "$SCRIPT" login wren
  test "$status" -eq 2
  out_has "wren runs as the Mac account chorus-wren"
  out_has "/etc/sudoers.d/chorus-principals"
  test -z "$(grep -F send-keys "$T/tmux.log" 2>/dev/null || true)"
}

@test "#4383 NEGATIVE PROOF: a principal whose account is the current user starts without sudo" {
  row wren "$USER"
  run "$SCRIPT" login wren
  test "$status" -eq 0
  test ! -f "$T/sudo.log"
  test -z "$(grep -F send-keys "$T/tmux.log" | grep -F "sudo -n" || true)"
}
