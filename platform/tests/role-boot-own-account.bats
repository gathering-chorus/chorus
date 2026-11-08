#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs (tmux, claude, token, curl, sudo, launchctl); no live services, no second account
# @domain: identity
#
# #4929 — Jeff 2026-11-08 06:26: "i expect the boot up to be the same". After the
# 06:3x power-off, `up` started Kade and Borg Queen as jeffbridwell (bash -lc, no
# sudo) while `login` started Silas, Wren and Abby in their own accounts: `up`
# called on() without the principal gate, so the role's account was never set.
# Every role now starts through one function, start_role, whichever verb runs it.
# Negative proof: run this file against a binary built before #4929
# (CHORUS_PRINCIPAL_TEST_BIN=<old build>) and the `up` cases go red.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # the three Claude roles only; Abby and Borg Queen start through the agent channel
  printf '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"}]}\n' > "$T/roles-door.json"
  for r in kade silas wren; do row "$r" "chorus-$r"; done
}
row() { printf '{"data":{"principalKind":"agent","hostAccount":"%s"}}\n200\n' "$2" > "$T/principal-$1.json"; }
start_line() { grep -F "respawn-pane -k -t chorus-$1" "$T/tmux.log" | tail -1; }

@test "#4929 up starts every role in its own account" {
  run "$SCRIPT" up
  for r in kade silas wren; do
    start_line "$r" | grep -qF -- "-u chorus-$r -H bash -c" || { echo "$r started outside its account: $(start_line "$r")"; return 1; }
  done
}

@test "#4929 no role started by up runs as the invoking account" {
  run "$SCRIPT" up
  for r in kade silas wren; do
    line="$(start_line "$r")"
    test -n "$line" || { echo "$r: no start line"; return 1; }
    if printf '%s' "$line" | grep -qE "bash -lc cd '"; then echo "$r ran as the invoker: $line"; return 1; fi
  done
}

@test "#4929 up and login give a role the same start line" {
  run "$SCRIPT" up
  up_line="$(start_line kade | sed -E 's/-1a[0-9a-z]+//g')"
  : > "$T/tmux.log"
  run "$SCRIPT" login kade
  login_line="$(start_line kade | sed -E 's/-1a[0-9a-z]+//g')"
  test -n "$up_line"
  test "$(printf '%s' "$up_line" | grep -oE 'sudo -n -u chorus-kade -H bash -c|launchctl asuser [0-9]+')" = "$(printf '%s' "$login_line" | grep -oE 'sudo -n -u chorus-kade -H bash -c|launchctl asuser [0-9]+')"
}

@test "#4929 up at login still starts roles when nobody is signed in yet (roles-up runs before the Clearing)" {
  touch "$T/nobody-signed-in"
  run "$SCRIPT" up
  for r in kade silas wren; do
    start_line "$r" | grep -qF -- "-u chorus-$r -H bash -c" || { echo "$r did not start: $output"; return 1; }
  done
}

@test "#4929 NEGATIVE PROOF: login by hand still needs a signed-in person" {
  touch "$T/nobody-signed-in"
  run "$SCRIPT" login kade
  test "$status" -eq 2
  test -z "$(start_line kade)"
}
