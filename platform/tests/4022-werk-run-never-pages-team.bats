#!/usr/bin/env bats
# @test-type: unit — hermetic: CHORUS_ROOT is an empty tmp tree (zero suites
# discovered), ops-nudge is a stub that records its argv, load is stubbed low,
# the reconciler binary is absent (typed unmeasured). Nothing live is touched.
#
# #4022 (#3722 follow-through) — a nightly launched from a card's werk must
# never page the team, WHATEVER the log path. On 2026-08-28 13:17 kade-4022's
# demo run was started with an explicit NIGHTLY_LOG_PATH; the isolation block
# keyed nudge-suppression on the log path being UNSET, so the run wrote its own
# log correctly and still fired "30 red across the board" at every role.
# Negative proof (#3734): the violating configuration, shown to page under the
# old coupling, is shown NOT to page now; the control shows a canonical-root
# run still pages (so the check cannot pass by ops-nudge being broken).

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  export NIGHTLY_LOCKDIR="$BATS_TEST_TMPDIR/lock.d"
  export CHORUS_LOG_BIN="$BATS_TEST_TMPDIR/chorus-log-stub"
  printf '#!/bin/bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
  export OPS_NUDGE="$BATS_TEST_TMPDIR/ops-nudge-stub"
  printf '#!/bin/bash\necho "$@" >> "%s/nudges.txt"\n' "$BATS_TEST_TMPDIR" > "$OPS_NUDGE"; chmod +x "$OPS_NUDGE"
  export NIGHTLY_LOAD_STUB=0.1
  export NIGHTLY_LOAD_DEFER_SECS=0
  export NIGHTLY_LOAD_RECHECK_SECS=1
  export NIGHTLY_RECONCILE_BIN=/nonexistent/werk-test
  export HOME="$BATS_TEST_TMPDIR/home"; mkdir -p "$HOME"
}

@test "negative proof: werk root + EXPLICIT log path — the team is NOT paged" {
  export CHORUS_ROOT="$BATS_TEST_TMPDIR/chorus-werk/kade-4022"; mkdir -p "$CHORUS_ROOT"
  export NIGHTLY_LOG_PATH="$BATS_TEST_TMPDIR/my-own.log"
  unset NIGHTLY_NO_NUDGE
  run "$NIGHTLY" --run-all
  [ "$status" -eq 0 ]
  [[ "$output" == *"WERK RUN"*"team nudge suppressed"* ]]
  grep -q '^RUN|complete|' "$NIGHTLY_LOG_PATH"          # the run happened, in MY log
  [ ! -e "$BATS_TEST_TMPDIR/nudges.txt" ]                # and nobody was paged
}

@test "werk root + no log path — still isolated, still silent (the #3722 case holds)" {
  # a fixture werk name no real card will ever have: the auto-isolated path is
  # /tmp/nightly-<werk basename>.log, and this test must never write a live
  # card's demo log (it did, once, as kade-4022 — 290 phantom suites in 35s).
  export CHORUS_ROOT="$BATS_TEST_TMPDIR/chorus-werk/kade-bats-fixture-$$"; mkdir -p "$CHORUS_ROOT"
  unset NIGHTLY_LOG_PATH NIGHTLY_NO_NUDGE
  run "$NIGHTLY" --run-all
  [ "$status" -eq 0 ]
  [[ "$output" == *"isolated to /tmp/nightly-kade-bats-fixture-$$.log"* ]]
  [ ! -e "$BATS_TEST_TMPDIR/nudges.txt" ]
  rm -f "/tmp/nightly-kade-bats-fixture-$$.log"
}

@test "control: canonical root DOES page — the stub is live, so silence above is real" {
  export CHORUS_ROOT="$BATS_TEST_TMPDIR/chorus"; mkdir -p "$CHORUS_ROOT"
  export NIGHTLY_LOG_PATH="$BATS_TEST_TMPDIR/canon.log"
  unset NIGHTLY_NO_NUDGE
  run "$NIGHTLY" --run-all
  [ "$status" -eq 0 ]
  [ -s "$BATS_TEST_TMPDIR/nudges.txt" ]
  grep -q 'nightly' "$BATS_TEST_TMPDIR/nudges.txt"
}
