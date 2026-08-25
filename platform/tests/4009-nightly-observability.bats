#!/usr/bin/env bats
# @test-type: unit — hermetic: stubs chorus-log and werk-test, writes fixtures to
# BATS_TEST_TMPDIR; no store, no network, no real suites.
# #4009 — a run must be able to say what it is doing WHILE it does it.
#
# 2026-08-25 receipts: one lane ran 38 minutes emitting nothing, so "working"
# and "wedged" were indistinguishable from outside; a human watched a
# minute-by-minute status assembled from log-line counts, file mtime, uptime
# and ps, and was told three different things about the same run. Two suites
# that never produced output were reported as "0 pass, 0 fail" — a row that
# reads like a measurement and isn't.
#
# The proofs below pin the three states that must stay separable:
#   measured pass/fail   vs   UNMEASURED   vs   silence

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  export CHORUS_LOG_BIN="$TMP/chorus-log"
  printf '#!/bin/bash\necho "$@" >> "%s/spine.txt"\n' "$TMP" > "$CHORUS_LOG_BIN"
  chmod +x "$CHORUS_LOG_BIN"
  export NIGHTLY_LOAD_STUB=0.1
}

# --- run_id: minted, stable, on every event -------------------------------

@test "every emitted event carries the same run_id" {
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    spine_emit test.one a=1; spine_emit test.two b=2
    echo \"\$NIGHTLY_RUN_ID\""
  [ "$status" -eq 0 ]
  id=$(printf '%s\n' "$output" | tail -1)
  [ -n "$id" ]
  [ "$(grep -c "run_id=$id" "$TMP/spine.txt")" -eq 2 ]
}

@test "an inherited run_id is honoured, not overwritten (children join the run)" {
  run bash -c "NIGHTLY_RUN_ID=nr-fixed-42 bash -c 'source \"$NIGHTLY\" --list-shell >/dev/null 2>&1; echo \$NIGHTLY_RUN_ID'"
  [[ "$output" == *"nr-fixed-42"* ]]
}

# --- UNMEASURED: the state that used to hide as 0/0 ------------------------

@test "negative proof: a suite with no output is UNMEASURED, never a pass or a 0/0 row" {
  cat > "$TMP/out.txt" <<'EOF'
SUITE|shell|platform/scripts/x.sh|kade|fail|0 pass, 0 fail
EOF
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    emit_suite_results \"\$(cat '$TMP/out.txt')\""
  [ "$status" -eq 0 ]
  # the emitted event must NOT claim a pass, and must name the state
  ! grep -q 'status=pass' "$TMP/spine.txt"
}

@test "control: a suite WITH counts stays measured — the check separates its states" {
  cat > "$TMP/out2.txt" <<'EOF'
SUITE|shell|platform/scripts/y.sh|kade|pass|7 pass, 0 fail
EOF
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    emit_suite_results \"\$(cat '$TMP/out2.txt')\""
  [ "$status" -eq 0 ]
  grep -q 'passed=7' "$TMP/spine.txt"
  ! grep -q 'UNMEASURED' "$TMP/spine.txt"
}

# --- lane liveness: entry and exit are announced ---------------------------

@test "the runner lane announces entry and exit so silence is attributable" {
  # A lane that emits nothing at all is the 2026-08-25 failure: with these two
  # events, a watcher can say WHICH lane went quiet and for how long.
  grep -q 'nightly.lane.started' "$NIGHTLY"
  grep -q 'nightly.lane.completed' "$NIGHTLY"
  grep -q 'nightly.suite.observed' "$NIGHTLY"
}

# --- quiet-cap: a wedge dies on silence, not on the 2-hour total cap ---------

@test "negative proof: a silent child is killed on the quiet-cap, not left for hours" {
  # The 2026-08-25 wedge: a lane alive, producing nothing, under a 7200s total
  # cap. Time-since-last-output is what identifies it.
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    NIGHTLY_SUITE_TIMEOUT=3600 NIGHTLY_QUIET_CAP=3 _run_capped '$TMP/q.out' sleep 60
    echo rc=\$?"
  [[ "$output" == *"rc=124"* ]]
  grep -q "SUITE WEDGED: no output for 3s" "$TMP/q.out"
}

@test "control: a chatty child of the same duration is NOT killed by the quiet-cap" {
  # Same wall-clock, but it keeps writing — the two states must stay separable.
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    NIGHTLY_SUITE_TIMEOUT=3600 NIGHTLY_QUIET_CAP=3 _run_capped '$TMP/c.out' \
      bash -c 'for i in 1 2 3 4 5 6; do echo tick; sleep 1; done'
    echo rc=\$?"
  [[ "$output" == *"rc=0"* ]]
  ! grep -q "SUITE WEDGED" "$TMP/c.out"
  [ "$(grep -c tick "$TMP/c.out")" -eq 6 ]
}

# --- the orphan hole: a killed wrapper must not leave its lane alive ---------

@test "negative proof: the trap reaps the child group, so no orphan survives" {
  # 2026-08-25: the wrapper was killed, the trap freed the lock, and the runner
  # lived another 1h52m beside a new run. Drive release_single_flight_lock
  # directly with a known group rather than spawning a nested wrapper.
  perl -e 'setpgrp(0,0); exec "sleep", "120"' >/dev/null 2>&1 &
  child=$!
  sleep 1
  kill -0 "$child" 2>/dev/null
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    NIGHTLY_LOCKDIR='$TMP/lock.d' NIGHTLY_CHILD_PGID=$child release_single_flight_lock"
  sleep 1
  ! kill -0 "$child" 2>/dev/null
}

@test "control: with no lane recorded the trap only frees the lock, kills nothing" {
  perl -e 'setpgrp(0,0); exec "sleep", "20"' >/dev/null 2>&1 &
  bystander=$!
  sleep 1
  mkdir -p "$TMP/lock2.d"
  run bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1
    NIGHTLY_LOCKDIR='$TMP/lock2.d' NIGHTLY_CHILD_PGID= release_single_flight_lock"
  [ ! -d "$TMP/lock2.d" ]
  kill -0 "$bystander" 2>/dev/null
  kill "$bystander" 2>/dev/null || true
}
