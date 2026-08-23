#!/usr/bin/env bats
# @test-type: unit — hermetic; stubs cargo/chorus-log, asserts failure-log capture, no live server
# #3484 — a red suite must EXPLAIN ITSELF. Today the runner keeps rc but throws
# the failure OUTPUT away (line 247: "compile/run failure rc=N"), so every
# morning is a fresh re-diagnosis with the evidence already gone. These assert:
# (1) a failing suite's output is captured to a failure log, (2) the emitted
# test.suite.result carries a one-line reason from it, (3) a pass leaves none.
# Jeff 2026-06-20: ends the every-morning re-guess loop.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_FAIL_DIR="$TMP/failures"
  CRATE="$TMP/fake-crate"; mkdir -p "$CRATE"
  BIN="$TMP/bin"; mkdir -p "$BIN"
  CAP="$TMP/spine-capture.txt"
  STUB="$TMP/chorus-log"
  cat > "$STUB" <<EOF
#!/usr/bin/env bash
echo "\$@" >> "$CAP"
EOF
  chmod +x "$STUB"
}

# #3753: rewritten for #3974 — run_one_attempt retired; the lane routes through
# a stubbed werk-test and the capture is asserted on the fold's own output.
@test "a failing cargo suite captures its output to a failure log" {
  cat > "$BIN/werk-test" <<EOF
#!/usr/bin/env bash
echo "error[E0432]: unresolved import \`foo::bar\`"
echo "nightly-unit|cargo|fake-crate|fail|0 pass, 1 fail (compile rc=101)"
exit 1
EOF
  chmod +x "$BIN/werk-test"
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$SCRIPT" --run-one cargo fake-crate
  [ "$status" -eq 1 ]
  logp=$(bash -c "source '$SCRIPT'; _fail_log_path cargo platform/services/fake-crate")
  [ -f "$logp" ]
  run cat "$logp"
  [[ "$output" == *"unresolved import"* ]]
}

@test "emit carries a one-line reason from the failure log" {
  logp=$(bash -c "source '$SCRIPT'; _fail_log_path cargo /y/werk-merge")
  mkdir -p "$(dirname "$logp")"
  printf 'Compiling werk-merge\nerror[E0432]: unresolved import\n' > "$logp"
  CHORUS_LOG_BIN="$STUB" bash -c "source '$SCRIPT'; emit_suite_results \"\$1\"" _ \
    "SUITE|cargo|/y/werk-merge|silas|fail|suites: 0 ok, 1 failed (compile/run failure rc=101)"
  run cat "$CAP"
  [[ "$output" == *"reason="* ]]
  [[ "$output" == *"unresolved import"* ]]
}

@test "a passing suite clears any stale failure log and emits no reason" {
  cat > "$BIN/werk-test" <<EOF
#!/usr/bin/env bash
echo "nightly-unit|cargo|fake-crate|pass|1 pass, 0 fail"
exit 0
EOF
  chmod +x "$BIN/werk-test"
  logp=$(bash -c "source '$SCRIPT'; _fail_log_path cargo platform/services/fake-crate")
  mkdir -p "$(dirname "$logp")"; echo "stale error" > "$logp"   # pre-existing
  NIGHTLY_LOAD_STUB=0.1 PATH="$BIN:$PATH" run "$SCRIPT" --run-one cargo fake-crate
  [ "$status" -eq 0 ]
  [ ! -f "$logp" ]   # green run removed the stale log
}
