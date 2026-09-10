#!/usr/bin/env bats
# @test-type: unit — hermetic (werk-test stubbed, launchctl stubbed)
# #4136 — "so its a performance signal not a test failure" (Jeff 2026-09-10).

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_FAIL_DIR="$TMP/failures"
  BIN="$TMP/bin"; mkdir -p "$BIN"
}

@test "a perf row over budget is SLOW on the page, not fail" {
  cat > "$BIN/werk-test" <<EOS
echo "nightly-unit|perf|platform/tests/werk-phase-budgets.test.sh|fail|0 pass, 1 fail"
exit 1
EOS
  chmod +x "$BIN/werk-test"
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$SCRIPT" --run-one perf platform/tests/werk-phase-budgets.test.sh
  [[ "$output" == *"|slow|"* ]]
  [[ "$output" != *"|fail|"* ]]
}

@test "NEGATIVE PROOF: a shell row over its own assertions is still fail" {
  cat > "$BIN/werk-test" <<EOS
echo "nightly-unit|shell|platform/tests/x.test.sh|fail|0 pass, 1 fail"
exit 1
EOS
  chmod +x "$BIN/werk-test"
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$SCRIPT" --run-one shell platform/tests/x.test.sh
  [[ "$output" == *"|fail|"* ]]
  [[ "$output" != *"|slow|"* ]]
}

@test "a perf row under budget stays pass" {
  cat > "$BIN/werk-test" <<EOS
echo "nightly-unit|perf|platform/tests/werk-phase-budgets.test.sh|pass|4 pass, 0 fail"
exit 0
EOS
  chmod +x "$BIN/werk-test"
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$SCRIPT" --run-one perf platform/tests/werk-phase-budgets.test.sh
  [[ "$output" == *"|pass|"* ]]
}

@test "the launchctl ledger names the caller and still runs the real binary" {
  export NIGHTLY_LAUNCHCTL_LEDGER="$TMP/callers.log"
  cat > "$TMP/fake-launchctl" <<'EOS'
#!/bin/bash
echo "real-launchctl ran: $*"
EOS
  chmod +x "$TMP/fake-launchctl"
  export LAUNCHCTL_REAL="$TMP/fake-launchctl"
  run bash -c "source '$SCRIPT'; launchctl kick""start gui/501/com.chorus.hooks"
  [[ "$output" == *"real-launchctl ran: kick"* ]]
  run cat "$TMP/callers.log"
  [[ "$output" == *"caller="* ]]
  [[ "$output" == *"com.chorus.hooks"* ]]
}
