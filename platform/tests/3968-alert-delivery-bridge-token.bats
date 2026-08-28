#!/usr/bin/env bats
# @test-type: integration — posts real probes to the live bridge (:3475); skip-if-absent
# #3968 — alert-delivery-test's bridge probe presents the BRIDGE_TOKEN (#3966
# closed anonymous posts). Negative proof per #3734: with no token the check
# FAILS with the real 401 — proving the gate can still distinguish the two
# states, and that the "HTTP 401000" concatenation artifact is gone.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/alert-delivery-test.sh"

setup() {
  # A test brings its own world (#3528): never write fixture results into the real log.
  export ALERT_DELIVERY_LOG="$BATS_TEST_TMPDIR/alert-delivery-test.log"
  curl -s -o /dev/null --max-time 3 http://localhost:3475/health || skip "bridge not running"
}

@test "with the real token: bridge probe check PASSes" {
  run bash "$SCRIPT"
  [[ "$output" == *"PASS: alert-runner: Bridge accepted probe"* ]]
}

# #4004 — the failure must NAME which state it is in. #3968 proved an absent
# token still reds; it reported that red as "Bridge rejected probe", so the
# alarm read as a delivery outage while delivery was fine (six such lines since
# 2026-08-24, interleaved with runs passing 6/6 seconds apart). The bridge is
# never asked when we have no credential, so it must not be the thing blamed.
@test "NEGATIVE PROOF: token missing → check FAILS naming the CREDENTIAL state, not the bridge" {
  BRIDGE_TOKEN_FILE="$BATS_TEST_TMPDIR/absent-token" run bash "$SCRIPT"
  [[ "$output" == *"FAIL: alert-runner: NO BRIDGE CREDENTIAL"* ]]
  [[ "$output" == *"$BATS_TEST_TMPDIR/absent-token"* ]]
  # the misattribution this replaces, and the artifact #3968 killed
  [[ "$output" != *"Bridge rejected probe"* ]]
  [[ "$output" != *"401000"* ]]
}

@test "NEGATIVE PROOF: an EMPTY token file is the same state as a missing one" {
  : > "$BATS_TEST_TMPDIR/empty-token"
  BRIDGE_TOKEN_FILE="$BATS_TEST_TMPDIR/empty-token" run bash "$SCRIPT"
  [[ "$output" == *"FAIL: alert-runner: NO BRIDGE CREDENTIAL"* ]]
}

@test "NEGATIVE PROOF: a WRONG token still blames the bridge — the two states stay separable" {
  printf 'not-the-real-token\n' > "$BATS_TEST_TMPDIR/wrong-token"
  BRIDGE_TOKEN_FILE="$BATS_TEST_TMPDIR/wrong-token" run bash "$SCRIPT"
  [[ "$output" == *"FAIL: alert-runner: Bridge rejected probe (HTTP 401)"* ]]
  [[ "$output" != *"NO BRIDGE CREDENTIAL"* ]]
}

# #4027 — a transport miss (000) is not the bridge's verdict. Under load the
# probe read 000 against a bridge answering /health in 11ms, and the log said
# "rejected". Point the probe at a closed port: the check must still FAIL, and
# name UNREACHABLE, never "rejected".
@test "NEGATIVE PROOF: bridge unreachable → check FAILS naming TRANSPORT, not a rejection" {
  BRIDGE="http://127.0.0.1:1" run bash "$SCRIPT"
  [[ "$output" == *"FAIL: alert-runner: Bridge UNREACHABLE"* ]]
  [[ "$output" != *"Bridge rejected probe"* ]]
}
