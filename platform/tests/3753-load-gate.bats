#!/usr/bin/env bats
# @test-type: unit — hermetic: load is stubbed via NIGHTLY_LOAD_STUB, log/lock
# paths live under $BATS_TEST_TMPDIR; no real suites run (defer window 0).
# #3753 — nightly load gate: negative proof (#3734) both directions.
#
# The gate exists to separate two states: "the code failed" vs "the machine
# was busy". Each test below pins one side so the check can never pass
# vacuously: stubbed HIGH load must HOLD the run (typed UNMEASURABLE, exit 0,
# no suites executed); stubbed LOW load must let it run.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  export NIGHTLY_LOG_PATH="$BATS_TEST_TMPDIR/nightly.log"
  export NIGHTLY_LOCKDIR="$BATS_TEST_TMPDIR/lock.d"
  export NIGHTLY_NO_NUDGE=1
  export CHORUS_LOG_BIN="$BATS_TEST_TMPDIR/chorus-log-stub"
  printf '#!/bin/bash\necho "$@" >> "%s/spine.txt"\n' "$BATS_TEST_TMPDIR" > "$CHORUS_LOG_BIN"
  chmod +x "$CHORUS_LOG_BIN"
  export NIGHTLY_LOAD_DEFER_SECS=0   # no waiting in tests
  export NIGHTLY_LOAD_RECHECK_SECS=1
}

@test "negative proof: high stubbed load HOLDS the run — typed UNMEASURABLE, exit 0, zero suites" {
  export NIGHTLY_LOAD_STUB=999
  run "$NIGHTLY" --run-all
  [ "$status" -eq 0 ]
  [[ "$output" == *"UNMEASURABLE"* ]]
  grep -q '^RUN|unmeasurable|' "$NIGHTLY_LOG_PATH"
  # spine carries the typed state, not a red
  grep -q 'nightly.run.unmeasurable' "$BATS_TEST_TMPDIR/spine.txt"
  grep -q 'zero_red=unmeasurable' "$BATS_TEST_TMPDIR/spine.txt"
  # no suite executed under load
  ! grep -q '^SUITE|' "$NIGHTLY_LOG_PATH"
}

@test "quiet box: low stubbed load passes the gate (--load-gate rc 0)" {
  NIGHTLY_LOAD_STUB=0.1 run "$NIGHTLY" --load-gate
  [ "$status" -eq 0 ]
  [[ "$output" == load=0.1* ]]
}

@test "threshold is cores-relative and config-overridable" {
  NIGHTLY_LOAD_STUB=999 NIGHTLY_LOAD_MAX_PER_CORE=0.1 run "$NIGHTLY" --load-gate
  [ "$status" -eq 1 ]   # 999 > cores*0.1 on any real box
  NIGHTLY_LOAD_STUB=5 NIGHTLY_LOAD_MAX_PER_CORE=100 run "$NIGHTLY" --load-gate
  [ "$status" -eq 0 ]   # 5 < cores*100 anywhere
}

# --- AC2: failed-to-START classes fold to unmeasurable, real fails stay fail ---

@test "AC2: spawn/ABI failure folds to unmeasurable; real assertion failure stays fail" {
  NIGHTLY_LOAD_STUB=0.1 run "$NIGHTLY" --classify fail "0 pass, 1 fail (npx jest: NODE_MODULE_VERSION 131 mismatch)"
  [ "$output" = "unmeasurable" ]
  NIGHTLY_LOAD_STUB=0.1 run "$NIGHTLY" --classify fail "3 pass, 2 fail (assertion errors)"
  [ "$output" = "fail" ]
  NIGHTLY_LOAD_STUB=0.1 run "$NIGHTLY" --classify pass "5 pass, 0 fail"
  [ "$output" = "pass" ]
}

@test "AC2: timeout folds to unmeasurable ONLY under load — quiet-box timeout stays fail" {
  NIGHTLY_LOAD_STUB=999 run "$NIGHTLY" --classify fail "0 pass, 1 fail (SUITE TIMEOUT: killed after 1800s)"
  [ "$output" = "unmeasurable" ]
  NIGHTLY_LOAD_STUB=0.1 run "$NIGHTLY" --classify fail "0 pass, 1 fail (SUITE TIMEOUT: killed after 1800s)"
  [ "$output" = "fail" ]
}

# --- AC3: probe/health timeout class downgrades ONLY under load ---

@test "AC3 negative proof: loaded box downgrades timeout-class failures to WARN; real errors stay FAIL" {
  run bash -c 'printf "%s\n" \
    "gathering-app: localhost:3002 returned 000 — app down" \
    "chorus-api: DOWN — code=500 exit=0 after retry" \
    | NIGHTLY_LOAD_STUB=999 "'"$BATS_TEST_DIRNAME"'/../scripts/load-reclassify.sh"'
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == "WARN|unmeasurable under load (load=999"* ]]
  [[ "${lines[1]}" == "FAIL|chorus-api: DOWN — code=500"* ]]
}

@test "AC3 negative proof: quiet box keeps timeouts as FAIL — the alert still fires" {
  run bash -c 'printf "%s\n" \
    "gathering-app: localhost:3002 returned 000 — app down" \
    | NIGHTLY_LOAD_STUB=0.1 "'"$BATS_TEST_DIRNAME"'/../scripts/load-reclassify.sh"'
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == "FAIL|gathering-app"* ]]
}
