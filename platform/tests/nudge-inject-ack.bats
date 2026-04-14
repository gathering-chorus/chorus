#!/usr/bin/env bats
# nudge-inject-ack.bats — verify nudge stdout reflects inject success/failure (#2036)
#
# Bug: nudge prints "DELIVERED" to stdout even when inject fails (TCC revoked).
# Clearing reads stdout, sees "DELIVERED", tells Jeff "Sent". Jeff thinks it worked.
# Fix: inject failure prints "INJECT_FAILED" not "DELIVERED".

NUDGE_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge"

@test "successful inject says DELIVERED (not INJECT_FAILED)" {
  # Nudge to a role with an active terminal — should succeed
  result=$(bash "$NUDGE_SCRIPT" kade "[bats-test] ack test" --force 2>/dev/null)
  echo "$result" | grep -q "DELIVERED"
  ! echo "$result" | grep -q "INJECT_FAILED"
}

@test "inject failure does NOT say DELIVERED in stdout" {
  # Check the source code — inject failure path's println must not contain "DELIVERED"
  SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/nudge.rs"
  inject_fail_block=$(grep -A5 'mode = "inject-failed-queued"' "$SRC" | grep 'println!' | grep -v 'eprintln!')
  ! echo "$inject_fail_block" | grep -q "DELIVERED"
}

@test "inject failure says INJECT_FAILED in stdout" {
  SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/nudge.rs"
  inject_fail_block=$(grep -A5 'mode = "inject-failed-queued"' "$SRC" | grep 'println!' | grep -v 'eprintln!')
  echo "$inject_fail_block" | grep -q "INJECT_FAILED"
}
