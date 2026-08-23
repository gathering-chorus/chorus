#!/usr/bin/env bats
# @test-type: integration — posts real probes to the live bridge (:3475); skip-if-absent
# #3968 — alert-delivery-test's bridge probe presents the BRIDGE_TOKEN (#3966
# closed anonymous posts). Negative proof per #3734: with no token the check
# FAILS with the real 401 — proving the gate can still distinguish the two
# states, and that the "HTTP 401000" concatenation artifact is gone.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/alert-delivery-test.sh"

setup() {
  curl -s -o /dev/null --max-time 3 http://localhost:3475/health || skip "bridge not running"
}

@test "with the real token: bridge probe check PASSes" {
  run bash "$SCRIPT"
  [[ "$output" == *"PASS: alert-runner: Bridge accepted probe"* ]]
}

@test "NEGATIVE PROOF: token missing → check FAILS with the real 401 (not 401000, not silent pass)" {
  BRIDGE_TOKEN_FILE="$BATS_TEST_TMPDIR/absent-token" run bash "$SCRIPT"
  [[ "$output" == *"FAIL: alert-runner: Bridge rejected probe (HTTP 401)"* ]]
  [[ "$output" != *"401000"* ]]
}
