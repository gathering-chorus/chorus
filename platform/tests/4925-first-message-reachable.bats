#!/usr/bin/env bats
# @test-type: integration — the principal's first-message rule (cargo), its sweep wiring, and the login path with the harness's stubs; no live services
# @domain: identity
#
# #4925 round 5 (Jeff 11-23 09:13 via Wren: the role gets its first message, 4895 stays
# read-only; Silas 10:29: from system, by the sweep, once per run, after the pane check).
# A Presence starts reachability "unknown" and only a delivered turn makes it reachable
# (rows.rs is_delivery). Abby and BQ restarted on 11-22 night, nobody nudged them, and 4895
# read them red. The ~60 s sweep (it runs as Jeff, who holds every role's tmux) now sends a
# role whose Presence is still unknown one message from system. Jeff's account never speaks
# as the role (that would be a spoof), and login sends nothing: at login the pane may not be
# corrected yet (#4813: typing into a pane mid-launch left the shell at `dquote>`).
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PR="$ROOT/platform/services/chorus-principal"
LIB="$PR/src/lib.rs"

# prints the body of fn $2 in the Rust file $1
fn_body() { awk -v f="fn $2(" 'index($0,f){p=1} p{print} p&&/^}/{exit}' "$1"; }
# the sweep's pane refresh asks the rule, then sends through first_message
wired() { fn_body "$1" refresh_pane | grep -q 'rows::first_message_due(&p' && fn_body "$1" refresh_pane | grep -q 'first_message(ctx, role, &run)'; }
# the message goes out from system, never under the role's name
from_system() { fn_body "$1" first_message | grep -qF '["nudge-send", "system", role, text]'; }

@test "#4925 the rule: an unknown Presence is due once per run; a reached one, or one with no run, never" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PR/target/$(id -un)" nice cargo test -q --manifest-path "$PR/Cargo.toml" --lib first_message_4925
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "2 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#4925 the sweep's pane refresh sends the first message, from system" {
  wired "$LIB" || { fn_body "$LIB" refresh_pane; return 1; }
  from_system "$LIB" || { fn_body "$LIB" first_message; return 1; }
}

@test "#4925 NEGATIVE: a refresh that never asks the rule, or a send under the role's name, is caught" {
  f="$BATS_TEST_TMPDIR/lib.rs"
  printf 'fn refresh_pane(ctx: &Ctx, role: &str) {\n    repane(ctx, role, p, &pane, &tty);\n}\nfn first_message(ctx: &Ctx, role: &str, run: &str) -> bool {\n    let args = ["nudge-send", role, role, text];\n}\n' > "$f"
  if wired "$f"; then echo "an unwired refresh passed"; return 1; fi
  if from_system "$f"; then echo "a send as the role passed"; return 1; fi
}

@test "#4925 NEGATIVE: a login sends no first message (the sweep does, after the pane check)" {
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/target/$(id -un)/release/chorus-principal}"
  [ -x "$SCRIPT" ] || { echo "chorus-principal not built at $SCRIPT"; return 1; }
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  est wren
  "$SCRIPT" login wren >/dev/null 2>&1
  grep -q "^session.run.recorded wren" "$T/spine.log"   # control: the login did record a run
  test ! -s "$T/nudge.log"
  if grep -q "presence.first_message" "$T/spine.log"; then false; fi
}
