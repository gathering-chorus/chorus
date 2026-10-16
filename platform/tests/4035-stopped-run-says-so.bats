#!/usr/bin/env bats
# @test-type: integration — drives the built werk-test binary with a stub runner that sleeps; signals it; no live service
# @domain: tests — the product domain this suite guards (#4334)
#
# #4035 → #4145 → #4540: a stop mid-run reaps the runner child and exits 143
# (TERM) / 130 (INT); the dagu nightly's abort handler then runs
# `report --stopped`, which writes `RUN|stopped|<ts>|signal=dagu …`. The page
# reads RUN|stopped as "this run never finished", never as silence.

setup() {
  BIN="${WERK_TEST_BIN:-$BATS_TEST_DIRNAME/../services/werk-test/target/release/werk-test}"
  [ -x "$BIN" ] || skip "werk-test not built at $BIN"
  T="$BATS_TEST_TMPDIR"
  # #3528 — bring your own process table: a real nightly on the box must not refuse these runs
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\n' > "$BATS_TEST_TMPDIR/ps-none"; chmod +x "$BATS_TEST_TMPDIR/ps-none"
  export WERK_TEST_PS="$BATS_TEST_TMPDIR/ps-none"
  mkdir -p "$T/root/platform/tests"
  # #4416 — the stub IS its sleep (exec, argv0 kept so pgrep finds it): when werk-test
  # reaps the runner the sleep dies with it. It was `sleep 300` in a bash child that
  # outlived the reap and held bats open: ~10 min per nightly for a 1.6 s suite.
  printf '#!/bin/bash\necho "nightly-plan|bats|x"\nexec -a "$0" sleep 20\n' > "$T/runner.sh"; chmod +x "$T/runner.sh"
  printf '#!/bin/bash\nexit 0\n' > "$T/nudge.sh"; chmod +x "$T/nudge.sh"
  # #4536 — the unseen-rows seed, stubbed: it records that it was called
  mkdir -p "$T/root/platform/scripts"
  printf '#!/bin/bash\necho "$*" >> "%s/seed-calls"\n' "$T" > "$T/root/platform/scripts/test-latest-seed"
  chmod +x "$T/root/platform/scripts/test-latest-seed"
}

env_steps() {
  CHORUS_ROOT="$T/root" CHORUS_HOME="$T/root" WERK_TEST_LOG_PATH="$T/run.log" WERK_TEST_FAIL_DIR="$T/fail" \
  CHORUS_TRACE_ID=034gpStopProof4035abcd WERK_TEST_RUNS="$T/runs" \
  OWLAPI=http://127.0.0.1:9 WERK_TEST_API=http://127.0.0.1:9 OPS_NUDGE="$T/nudge.sh" WERK_TEST_RUNNER_CMD="$T/runner.sh" \
  WERK_TEST_LEGS_NOOP=1 WERK_TEST_LOAD_MAX_PER_CORE=99 CHORUS_LOG_BIN=/nonexistent exec "$BIN" "$@"
}

start_run() {
  ( env_steps scope --scope=all >/dev/null 2>&1 )
  # exec: RUN_PID is werk-test itself, the process dagu signals
  ( env_steps run --scope=all >/dev/null 2>&1 ) &
  RUN_PID=$!
  for _ in $(seq 1 100); do pgrep -f "$T/runner.sh" >/dev/null && break; sleep 0.1; done
}

@test "negative proof: TERM mid-run reaps the runner, exits 143, and the abort handler's report writes RUN|stopped" {
  start_run
  child=$(pgrep -f "$T/runner.sh" | head -1); [ -n "$child" ]
  kill -TERM "$RUN_PID"
  wait "$RUN_PID" || status=$?
  [ "${status:-0}" -eq 143 ]
  sleep 0.5
  ! kill -0 "$child" 2>/dev/null || return 1
  ( env_steps report --scope=all --stopped >/dev/null 2>&1 )
  grep -q '^RUN|stopped|' "$T/run.log"
  ! grep -q '^RUN|complete|' "$T/run.log" || return 1
  # #4536 NEGATIVE PROOF (Silas 08:33): a stopped run marks no Test row
  # "did not run" — it would blank thousands of good results
  [ ! -e "$T/seed-calls" ] || { cat "$T/seed-calls"; return 1; }
}

@test "INT exits 130 and reaps the runner" {
  start_run
  child=$(pgrep -f "$T/runner.sh" | head -1); [ -n "$child" ]
  kill -INT "$RUN_PID"
  wait "$RUN_PID" || status=$?
  [ "${status:-0}" -eq 130 ]
  sleep 0.5
  ! kill -0 "$child" 2>/dev/null || return 1
}

@test "control: a run that finishes writes RUN|complete and no RUN|stopped" {
  printf '#!/bin/bash\n[ "$WERK_TEST_STAGES" = perf ] && exit 0\necho "nightly-unit|bats|platform/tests/x.bats|pass|1 pass, 0 fail"\n' > "$T/runner.sh"
  for s in scope prepare run report; do ( env_steps "$s" --scope=all >/dev/null 2>&1 ); done
  grep -q '^RUN|complete|' "$T/run.log"
  ! grep -q '^RUN|stopped|' "$T/run.log" || return 1
  # #4536 — a COMPLETED run marks the rows it never ran, once, with its trace
  [ "$(wc -l < "$T/seed-calls" | tr -d ' ')" -eq 1 ]
  grep -qE '^034gpStopProof4035abcd --unseen-only --apply$' "$T/seed-calls" || { cat "$T/seed-calls"; return 1; }
}
