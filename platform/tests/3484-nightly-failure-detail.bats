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

@test "a failing cargo suite captures its output to a failure log" {
  cat > "$BIN/cargo" <<EOF
#!/usr/bin/env bash
echo "error[E0432]: unresolved import \`foo::bar\`" >&2
exit 101
EOF
  chmod +x "$BIN/cargo"
  PATH="$BIN:$PATH" bash -c "source '$SCRIPT'; run_one_attempt cargo '$CRATE' silas" >/dev/null
  logp=$(bash -c "source '$SCRIPT'; _fail_log_path cargo '$CRATE'")
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
  cat > "$BIN/cargo" <<EOF
#!/usr/bin/env bash
echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
exit 0
EOF
  chmod +x "$BIN/cargo"
  logp=$(bash -c "source '$SCRIPT'; _fail_log_path cargo '$CRATE'")
  mkdir -p "$(dirname "$logp")"; echo "stale error" > "$logp"   # pre-existing
  PATH="$BIN:$PATH" bash -c "source '$SCRIPT'; run_one_attempt cargo '$CRATE' silas" >/dev/null
  [ ! -f "$logp" ]   # green run removed the stale log
}
