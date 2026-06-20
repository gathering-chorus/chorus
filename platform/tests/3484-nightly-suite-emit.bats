#!/usr/bin/env bats
# @test-type: unit — hermetic; stubs chorus-log, drives emit_suite_results directly, no live server
# #3484 — the nightly must EMIT a structured, queryable per-suite result for
# EVERY suite (green and red), not just a count nudge + raw stdout. Jeff
# 2026-06-20: "we need to emit logs to show which test sets pass and fail."
# These assert emit_suite_results turns each SUITE| line into one
# `test.suite.result` spine event carrying suite/kind/status/passed/failed.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  CAP="$BATS_TEST_TMPDIR/spine-capture.txt"
  STUB="$BATS_TEST_TMPDIR/chorus-log"
  cat > "$STUB" <<EOF
#!/usr/bin/env bash
echo "\$@" >> "$CAP"
EOF
  chmod +x "$STUB"
}

# Source the script (guarded so sourcing only imports functions) with the
# chorus-log stub wired in, then call the emitter.
run_emit() {
  CHORUS_LOG_BIN="$STUB" bash -c "source '$SCRIPT'; emit_suite_results \"\$1\"" _ "$1"
}

@test "every suite emits one test.suite.result event (green AND red)" {
  local results="SUITE|bats|/x/alert-delivery.bats|silas|fail|bats: 10 passed, 4 failed
SUITE|cargo|/y/werk-push|silas|pass|suites: 5 ok, 0 failed"
  run_emit "$results"
  # exactly two events — one per suite, greens included
  run grep -c "test.suite.result" "$CAP"
  [ "$output" -eq 2 ]
}

@test "red suite carries status=fail with real passed/failed counts" {
  run_emit "SUITE|bats|/x/alert-delivery.bats|silas|fail|bats: 10 passed, 4 failed"
  run cat "$CAP"
  [[ "$output" == *"test.suite.result"* ]]
  [[ "$output" == *"suite=alert-delivery.bats"* ]]
  [[ "$output" == *"status=fail"* ]]
  [[ "$output" == *"passed=10"* ]]
  [[ "$output" == *"failed=4"* ]]
}

@test "green suite emits status=pass (greens are visible, not just reds)" {
  run_emit "SUITE|cargo|/y/werk-push|silas|pass|suites: 5 ok, 0 failed"
  run cat "$CAP"
  [[ "$output" == *"status=pass"* ]]
  [[ "$output" == *"suite=werk-push"* ]]
  [[ "$output" == *"passed=5"* ]]
  [[ "$output" == *"failed=0"* ]]
}

@test "false-mass-red signature (compile/run failure) still emits a parseable event" {
  run_emit "SUITE|cargo|/y/werk-merge|silas|fail|suites: 0 ok, 1 failed (compile/run failure rc=2)"
  run cat "$CAP"
  [[ "$output" == *"status=fail"* ]]
  [[ "$output" == *"passed=0"* ]]
  [[ "$output" == *"failed=1"* ]]
}

@test "cucumber summary parses by label, not position (60 passed / 45 failed, not 110)" {
  run_emit "SUITE|cucumber|/z/platform/tests|silas|fail|110 scenarios (45 failed, 5 undefined, 60 passed)"
  run cat "$CAP"
  [[ "$output" == *"passed=60"* ]]
  [[ "$output" == *"failed=45"* ]]
  [[ "$output" != *"passed=110"* ]]
}
