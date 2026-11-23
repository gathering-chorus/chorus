#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4925 round 5 (Jeff 11-23 09:13 via Wren: "login sends the role its first message",
# 4895 stays read-only). A Presence starts reachability "unknown" and only a delivered
# turn makes it reachable (rows.rs is_delivery). Abby and BQ restarted on 11-22 night,
# nobody nudged them, and 4895 read them red. A new run now sends the role one message
# to itself (its own process group in prod; AWAKE_NUDGE_SYNC=1 here, so no race).
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
}
firsts() { [ -f "$T/nudge.log" ] || { echo 0; return; }; grep -c "^nudge-send wren wren \[presence\] new run" "$T/nudge.log" || true; }
turn() { echo '{"session_id":"c-4925","prompt":"hello"}' | AWAKE_SEEN_SYNC=1 AWAKE_SEEN_EVERY=0 "$SCRIPT" seen wren >/dev/null 2>&1; }

@test "#4925 NEGATIVE: a new run sends the role one first message, and a live run sends none" {
  # the login recorded wren's run: that new run asked for its first delivery, once
  test "$(firsts)" -eq 1
  grep -q "presence.first_message wren why=new run outcome=sent" "$T/spine.log"
  # turns on the live run record no new run, so they send nothing more
  turn
  turn
  test "$(firsts)" -eq 1
}
