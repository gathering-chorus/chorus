#!/usr/bin/env bats
# @test-type: unit — hermetic; sources nightly-suites.sh functions, own tmp log,
# a throwaway setpgrp'd sleeper stands in for the runner. Nothing live.
#
# #4035 — a stopped run says so, and takes its runner with it. 2026-08-31 03:21:
# agent-state stop killed the wrapper, the runner (own process group) ran on
# alone to 05:53, and the log kept a RUN|start with nothing after it — /nightly
# showed a blank morning. Negative proofs (#3734): the violating state (stop
# mid-run) is produced and shown to leave a RUN|stopped line and NO orphaned
# runner group; the control (no run open) writes nothing.

setup() {
  SCRIPT="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_LOG_PATH="$TMP/run.log"
  export NIGHTLY_LOCKDIR="$TMP/lock.d"; mkdir -p "$NIGHTLY_LOCKDIR"
}

@test "negative proof: stop mid-run writes RUN|stopped and reaps the runner's process group" {
  # a stand-in runner in its OWN process group, like _run_capped makes one
  perl -e 'setpgrp(0,0); exec "sleep", "300"' &
  runner=$!
  sleep 0.3
  kill -0 "$runner"  # alive before the stop
  run bash -c "source '$SCRIPT'; NIGHTLY_RUN_OPEN=1 NIGHTLY_CHILD_PGID=$runner _on_stop TERM"
  [ "$status" -eq 143 ]
  grep -q '^RUN|stopped|' "$NIGHTLY_LOG_PATH"
  grep -q 'signal=TERM' "$NIGHTLY_LOG_PATH"
  sleep 1.2   # release_single_flight_lock TERMs, waits 1s, KILLs
  ! kill -0 "$runner" 2>/dev/null   # no orphan
  [ ! -d "$NIGHTLY_LOCKDIR" ]       # lock freed
}

@test "control: no run open — nothing written, still reaps and frees the lock" {
  run bash -c "source '$SCRIPT'; NIGHTLY_RUN_OPEN=0 NIGHTLY_CHILD_PGID= _on_stop TERM"
  [ "$status" -eq 143 ]
  [ ! -f "$NIGHTLY_LOG_PATH" ]
  [ ! -d "$NIGHTLY_LOCKDIR" ]
}

@test "INT exits 130 and still writes the stop line" {
  run bash -c "source '$SCRIPT'; NIGHTLY_RUN_OPEN=1 NIGHTLY_CHILD_PGID= _on_stop INT"
  [ "$status" -eq 130 ]
  grep -q 'signal=INT' "$NIGHTLY_LOG_PATH"
}
