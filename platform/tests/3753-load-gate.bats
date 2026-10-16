#!/usr/bin/env bats
# @test-type: unit — hermetic: load is stubbed via WERK_TEST_LOAD_STUB, log/lock
# @domain: tests — the product domain this suite guards (#4334)
# paths live under $BATS_TEST_TMPDIR; no real suites run (defer window 0).
# #3753 — nightly load gate: negative proof (#3734) both directions.
#
# The gate exists to separate two states: "the code failed" vs "the machine
# was busy". Each test below pins one side so the check can never pass
# vacuously: stubbed HIGH load must HOLD the run (typed UNMEASURABLE, exit 0,
# no suites executed); stubbed LOW load must let it run.


setup() {
  BIN="${WERK_TEST_BIN:-$BATS_TEST_DIRNAME/../services/werk-test/target/release/werk-test}"
  [ -x "$BIN" ] || skip "werk-test not built at $BIN"
  export WERK_TEST_LOG_PATH="$BATS_TEST_TMPDIR/nightly.log"
  export WERK_TEST_LOCKDIR="$BATS_TEST_TMPDIR/lock.d"
  export WERK_TEST_NO_NUDGE=1
  export CHORUS_LOG_BIN="$BATS_TEST_TMPDIR/chorus-log-stub"
  printf '#!/bin/bash\necho "$@" >> "%s/spine.txt"\n' "$BATS_TEST_TMPDIR" > "$CHORUS_LOG_BIN"
  chmod +x "$CHORUS_LOG_BIN"
  export WERK_TEST_LOAD_DEFER_SECS=0   # no waiting in tests
  export WERK_TEST_LOAD_RECHECK_SECS=1
  # #3528 — bring your own process table: a real nightly on the box must not refuse these runs
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\n' > "$BATS_TEST_TMPDIR/ps-none"; chmod +x "$BATS_TEST_TMPDIR/ps-none"
  export WERK_TEST_PS="$BATS_TEST_TMPDIR/ps-none"
}

@test "negative proof: high stubbed load HOLDS the run — typed UNMEASURABLE, exit 0, zero suites" {
  export WERK_TEST_LOAD_STUB=999
  # #4540 — the nightly is dagu steps now: prepare holds, run and report see the hold
  run bash -c 'for s in scope prepare run report; do
    CHORUS_ROOT="$0" CHORUS_HOME="$0" CHORUS_TRACE_ID=bats-3753 WERK_TEST_RUNS="$0/runs" WERK_TEST_LOAD_DEFER_SECS=0 FUSEKI_QUERY=http://127.0.0.1:9/query \
    WERK_TEST_RUNNER_CMD=/nonexistent WERK_TEST_LEGS_NOOP=1 OWLAPI=http://127.0.0.1:9 WERK_TEST_API=http://127.0.0.1:9 \
    "$1" "$s" --scope=all 2>&1 || exit $?; done' "$BATS_TEST_TMPDIR" "$BIN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"UNMEASURABLE"* ]] || return 1
  grep -q '^RUN|unmeasurable|' "$WERK_TEST_LOG_PATH"
  # spine carries the typed state, not a red
  grep -q 'nightly.run.unmeasurable' "$BATS_TEST_TMPDIR/spine.txt"
  grep -q 'zero_red=unmeasurable' "$BATS_TEST_TMPDIR/spine.txt"
  # no suite executed under load
  ! grep -q '^SUITE|' "$WERK_TEST_LOG_PATH" || return 1
}

@test "quiet box: low stubbed load passes the gate (load-gate rc 0)" {
  WERK_TEST_LOAD_STUB=0.1 run "$BIN" load-gate
  [ "$status" -eq 0 ]
  [[ "$output" == load=0.1* ]] || return 1
}

@test "threshold is cores-relative and config-overridable" {
  WERK_TEST_LOAD_STUB=999 WERK_TEST_LOAD_MAX_PER_CORE=0.1 run "$BIN" load-gate
  [ "$status" -eq 1 ]   # 999 > cores*0.1 on any real box
  WERK_TEST_LOAD_STUB=5 WERK_TEST_LOAD_MAX_PER_CORE=100 run "$BIN" load-gate
  [ "$status" -eq 0 ]   # 5 < cores*100 anywhere
}

# --- AC2: failed-to-START classes fold to unmeasurable, real fails stay fail ---

@test "AC2: spawn/ABI failure folds to unmeasurable; real assertion failure stays fail" {
  WERK_TEST_LOAD_STUB=0.1 run "$BIN" classify fail "0 pass, 1 fail (npx jest: NODE_MODULE_VERSION 131 mismatch)"
  [ "$output" = "unmeasurable" ]
  WERK_TEST_LOAD_STUB=0.1 run "$BIN" classify fail "3 pass, 2 fail (assertion errors)"
  [ "$output" = "fail" ]
  WERK_TEST_LOAD_STUB=0.1 run "$BIN" classify pass "5 pass, 0 fail"
  [ "$output" = "pass" ]
}

# #4419 — Jeff: "any time u blame load u are almost always wrong". A timeout is
# red whatever the load; the busy-box fold to unmeasurable is gone.
@test "AC2: a suite timeout is fail whatever the load — busy box and quiet box alike" {
  WERK_TEST_LOAD_STUB=999 run "$BIN" classify fail "0 pass, 1 fail (SUITE TIMEOUT: killed after 1800s)"
  [ "$output" = "fail" ]
  WERK_TEST_LOAD_STUB=0.1 run "$BIN" classify fail "0 pass, 1 fail (SUITE TIMEOUT: killed after 1800s)"
  [ "$output" = "fail" ]
}

# --- AC3: probe/health timeout class downgrades ONLY under load ---

@test "AC3 negative proof: loaded box downgrades timeout-class failures to WARN; real errors stay FAIL" {
  run bash -c 'printf "%s\n" \
    "gathering-app: localhost:3002 returned 000 — app down" \
    "chorus-api: DOWN — code=500 exit=0 after retry" \
    | WERK_TEST_LOAD_STUB=999 LOAD_GATE_BIN="'"$BIN"'" "'"$BATS_TEST_DIRNAME"'/../scripts/load-reclassify.sh"'
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == "WARN|unmeasurable under load (load=999"* ]] || return 1
  [[ "${lines[1]}" == "FAIL|chorus-api: DOWN — code=500"* ]] || return 1
}

@test "AC3 negative proof: quiet box keeps timeouts as FAIL — the alert still fires" {
  run bash -c 'printf "%s\n" \
    "gathering-app: localhost:3002 returned 000 — app down" \
    | WERK_TEST_LOAD_STUB=0.1 LOAD_GATE_BIN="'"$BIN"'" "'"$BATS_TEST_DIRNAME"'/../scripts/load-reclassify.sh"'
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == "FAIL|gathering-app"* ]] || return 1
}
