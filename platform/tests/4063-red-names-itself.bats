#!/usr/bin/env bats
# @test-type: unit
# 4063-red-names-itself.bats — a nightly red must name its own cause.
#
# Two defects from the 2026-09-02 03:00 readout, both in how a red is folded:
#  1. "tests-domain: 515 registered tests never ran" read as a ledger defect
#     (Kade's) when those tests lived in platform/api, which the runner killed
#     at its 1200s cap (Silas's red). One cause, two reds, wrong owner.
#  2. Every runner-lane red carried, as its reason, the LAST error-ish line of
#     the shared lane log — an unrelated PASSING bats suite.
#
# NEGATIVE PROOFS (#3734): attribution must go to zero when no unit failed
# empty (never invent a dead unit), and the reason must never come from a line
# that names a different unit.

setup() {
  NIGHTLY="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/nightly-suites.sh"
  [ -f "$NIGHTLY" ] || skip "nightly-suites.sh not found"
  W="$BATS_TEST_TMPDIR"
  export NIGHTLY_FAIL_DIR="$W/fail"; mkdir -p "$NIGHTLY_FAIL_DIR"
  LANE="$NIGHTLY_FAIL_DIR/_lane-output.log"
  REC='reconcile: registered 7784, never-run (5):
  platform/api/tests/class-atlas-constraints-4053.test.ts :: a field carries its regex
  platform/api/tests/class-atlas-definitions-4053.test.ts :: an edge carries the definition
  platform/api/tests/athena.integration.test.ts :: actors returns 404
  directing/clearing/tests/one-door-redirect-3775.test.ts :: <button>Log in</button>
  platform/tests/x.bats :: something'
}

attribute() {
  bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _attribute_never_ran \"\$1\" \"\$2\"" _ "$1" "$2"
}

@test "never-run tests are attributed to the unit that produced no results, with its why" {
  printf '%s\n' \
    '!! jest:platform/api killed after 1200s — per-unit wall cap (NIGHTLY_UNIT_TIMEOUT, #4030)' \
    'nightly-unit|security|platform/api|fail|0 pass, 0 fail' \
    'nightly-unit|npm|directing/clearing|pass|852 pass, 0 fail' > "$LANE"
  run attribute "$REC" "$LANE"
  [[ "$output" == *"3 in units that produced no results this run (platform/api 3 [killed after 1200s])"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"2 unattributed"* ]]
  [[ "$output" == *"fix the unit, not the ledger"* ]]
}

@test "NEGATIVE PROOF: no unit failed empty — nothing is attributed, the ledger is named" {
  printf '%s\n' \
    'nightly-unit|security|platform/api|fail|2002 pass, 179 fail' \
    'nightly-unit|npm|directing/clearing|pass|852 pass, 0 fail' > "$LANE"
  run attribute "$REC" "$LANE"
  [[ "$output" != *"in units that produced no results"* ]]
  [[ "$output" == *"the ledger does not cross-foot"* ]]
}

@test "NEGATIVE PROOF: a missing lane log attributes nothing rather than guessing" {
  rm -f "$LANE"
  run attribute "$REC" "$LANE"
  [[ "$output" == *"the ledger does not cross-foot"* ]]
}

@test "the reconcile row carries the attribution end to end" {
  printf '%s\n' \
    '!! jest:platform/api killed after 1200s — per-unit wall cap (NIGHTLY_UNIT_TIMEOUT, #4030)' \
    'nightly-unit|security|platform/api|fail|0 pass, 0 fail' > "$LANE"
  printf '#!/bin/bash\ncat <<"EOF2"\n%s\nEOF2\nexit 0\n' "$REC" > "$W/werk-test"; chmod +x "$W/werk-test"
  run env NIGHTLY_RECONCILE_BIN="$W/werk-test" bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _reconcile_leg"
  [[ "$output" == *"SUITE|reconcile|tests-domain|kade|fail|"* ]]
  [[ "$output" == *"5 registered test(s) never ran of 7784"* ]]
  [[ "$output" == *"platform/api 3 [killed after 1200s]"* ]]
}

# --- the reason line ---

reason_for() {
  # emit_suite_results over one SUITE row with a stubbed spine_emit; print the reason= field
  local row="$1"
  EMITTED="$W/emitted.txt"; : > "$EMITTED"
  EMITTED="$EMITTED" NIGHTLY_FAIL_DIR="$NIGHTLY_FAIL_DIR" bash -c '
    source "'"$NIGHTLY"'" --list-shell >/dev/null 2>&1
    spine_emit() { printf "%s\n" "$*" >> "$EMITTED"; }
    emit_suite_results "$1"' _ "$row" >/dev/null 2>&1
  grep -o 'reason=.*' "$EMITTED" | head -1
}

@test "a runner-lane red names ITS failing case, not the shared log's last error line" {
  local flog; flog=$(bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _fail_log_path npm directing/clearing")
  printf '%s\n' \
    '# unit: directing/clearing (npm) — verdict fail' \
    '!! jest:directing/clearing FAILED: directing/clearing/tests/scroll.test.ts :: pinned-to-bottom follow' \
    'nightly-unit|npm|directing/clearing|fail|852 pass, 1 fail' \
    '--- error lines from this lane ---' \
    'nightly-unit|bats|platform/tests/crawler-error-tracking.bats|pass|4 pass, 0 fail' > "$flog"
  run reason_for 'SUITE|npm|directing/clearing|kade|fail|852 pass, 1 fail'
  [[ "$output" == *"pinned-to-bottom follow"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"crawler-error-tracking"* ]]
}

@test "NEGATIVE PROOF: a FAILED line for ANOTHER unit is never borrowed as this unit's reason" {
  local flog; flog=$(bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _fail_log_path npm directing/clearing")
  printf '%s\n' \
    '!! jest:platform/api FAILED: platform/api/tests/z.test.ts :: some other red' \
    'assertion failed in clearing lane' > "$flog"
  run reason_for 'SUITE|npm|directing/clearing|kade|fail|852 pass, 1 fail'
  [[ "$output" != *"some other red"* ]]
  [[ "$output" == *"assertion failed in clearing lane"* ]]
}
