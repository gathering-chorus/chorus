#!/usr/bin/env bats
# @test-type: integration — drives the built werk-test binary with a stub runner that sleeps; signals it; no live service
#
# #4035 → #4145: a stop mid-run writes `RUN|stopped|<ts>|signal=… pid=…`, reaps
# the runner child, frees the lock, exits 143 (TERM) / 130 (INT). The page
# reads RUN|stopped as "this run never finished", never as silence.

setup() {
  BIN="${WERK_TEST_BIN:-$BATS_TEST_DIRNAME/../services/werk-test/target/release/werk-test}"
  [ -x "$BIN" ] || skip "werk-test not built at $BIN"
  T="$BATS_TEST_TMPDIR"
  # #3528 — bring your own process table: a real nightly on the box must not refuse these runs
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\n' > "$BATS_TEST_TMPDIR/ps-none"; chmod +x "$BATS_TEST_TMPDIR/ps-none"
  export NIGHTLY_PS="$BATS_TEST_TMPDIR/ps-none"
  mkdir -p "$T/root/platform/tests"
  printf '#!/bin/bash\necho "nightly-plan|bats|x"\nsleep 300\n' > "$T/runner.sh"; chmod +x "$T/runner.sh"
  printf '#!/bin/bash\nexit 0\n' > "$T/nudge.sh"; chmod +x "$T/nudge.sh"
}

start_run() {
  CHORUS_ROOT="$T/root" CHORUS_HOME="$T/root" NIGHTLY_LOG_PATH="$T/run.log" NIGHTLY_FAIL_DIR="$T/fail" NIGHTLY_LOCKDIR="$T/lock.d" \
  OWLAPI=http://127.0.0.1:9 NIGHTLY_API=http://127.0.0.1:9 OPS_NUDGE="$T/nudge.sh" NIGHTLY_RUNNER_CMD="$T/runner.sh" \
  NIGHTLY_LEGS_NOOP=1 NIGHTLY_LOAD_MAX_PER_CORE=99 CHORUS_LOG_BIN=/nonexistent "$BIN" --nightly --run-all >/dev/null 2>&1 &
  RUN_PID=$!
  for _ in $(seq 1 100); do grep -q '^RUN|start|' "$T/run.log" 2>/dev/null && pgrep -f "$T/runner.sh" >/dev/null && break; sleep 0.1; done
}

@test "negative proof: TERM mid-run writes RUN|stopped, reaps the runner, frees the lock, exits 143" {
  start_run
  child=$(pgrep -f "$T/runner.sh" | head -1); [ -n "$child" ]
  kill -TERM "$RUN_PID"
  wait "$RUN_PID" || status=$?
  [ "${status:-0}" -eq 143 ]
  grep -q '^RUN|stopped|' "$T/run.log"
  grep -q 'signal=TERM' "$T/run.log"
  sleep 0.5
  ! kill -0 "$child" 2>/dev/null
  [ ! -d "$T/lock.d" ]
}

@test "INT exits 130 and still writes the stop line" {
  start_run
  kill -INT "$RUN_PID"
  wait "$RUN_PID" || status=$?
  [ "${status:-0}" -eq 130 ]
  grep -q 'signal=INT' "$T/run.log"
}

@test "control: a run that finishes writes RUN|complete and no RUN|stopped" {
  printf '#!/bin/bash\necho "nightly-unit|bats|platform/tests/x.bats|pass|1 pass, 0 fail"\n' > "$T/runner.sh"
  CHORUS_ROOT="$T/root" CHORUS_HOME="$T/root" NIGHTLY_LOG_PATH="$T/run.log" NIGHTLY_FAIL_DIR="$T/fail" NIGHTLY_LOCKDIR="$T/lock.d" \
  OWLAPI=http://127.0.0.1:9 NIGHTLY_API=http://127.0.0.1:9 OPS_NUDGE="$T/nudge.sh" NIGHTLY_RUNNER_CMD="$T/runner.sh" \
  NIGHTLY_LEGS_NOOP=1 NIGHTLY_LOAD_MAX_PER_CORE=99 CHORUS_LOG_BIN=/nonexistent run "$BIN" --nightly --run-all
  grep -q '^RUN|complete|' "$T/run.log"
  ! grep -q '^RUN|stopped|' "$T/run.log"
}
