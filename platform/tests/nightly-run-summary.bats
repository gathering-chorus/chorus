#!/usr/bin/env bats
# @test-type: unit
# nightly-run-summary.bats (#3606) — the board-level aggregate.
#
# WHY THIS EXISTS. `notify_results` nudges each owner about THEIR red suites
# (#3254) and `emit_suite_results` emits one event per SUITE (#3484). Both are
# correctly scoped to a slice, and that is exactly the gap: nothing carried the
# run's own number.
#
# On 2026-08-04 the board was 31 red — silas 25, kade 6. The nudge kade received
# said "6 suite(s) red" and was CORRECT. Read alone it says the nightly is nearly
# fine. Two owners can each get an accurate nudge and both be misled about main.
#
# Jeff's bar is ZERO RED ACROSS THE BOARD. These tests hold the aggregate to that
# bar, in BOTH directions — the count must be right when red, and the green claim
# must be impossible to fabricate.

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/nightly-suites.sh"
  [ -f "$SCRIPT" ] || skip "nightly-suites.sh not found"
  STUB_DIR="$BATS_TEST_TMPDIR/stub"; mkdir -p "$STUB_DIR"
  EMITTED="$BATS_TEST_TMPDIR/emitted.txt"; : > "$EMITTED"
}

# Drive the real functions with a fabricated results block and a stubbed
# spine_emit, so the assertions are about the aggregation, not about Fuseki,
# Loki, or a live nightly.
# ORDER IS LOAD-BEARING: source FIRST, then override spine_emit. The script
# defines its own spine_emit, so a stub declared before the source is silently
# clobbered and every emission goes to the REAL spine — which is how the first
# version of this file produced a passing test with an empty capture file. A
# harness that cannot observe the thing it asserts on will report whatever the
# assertion's default is; that is the same defect this whole card is about, and
# it bit here first.
run_summary_over() {
  local results="$1"
  : > "$EMITTED"
  EMITTED="$EMITTED" bash -c '
    # shellcheck disable=SC1090
    source "'"$SCRIPT"'" 2>/dev/null || true
    spine_emit() { printf "%s\n" "$*" >> "$EMITTED"; }
    emit_run_summary "$1"
  ' _ "$results" 2>/dev/null || true
  cat "$EMITTED"
}

MIXED='SUITE|bats|/a/one.bats|kade|pass|bats: 3 passed, 0 failed
SUITE|bats|/a/two.bats|kade|fail|bats: 1 passed, 2 failed
SUITE|bats|/a/three.bats|silas|fail|bats: 0 passed, 1 failed
SUITE|bats|/a/four.bats|silas|fail|bats: 0 passed, 4 failed
SUITE|bats|/a/five.bats|silas|skip|no live stack'

ALL_GREEN='SUITE|bats|/a/one.bats|kade|pass|bats: 3 passed, 0 failed
SUITE|bats|/a/two.bats|silas|pass|bats: 5 passed, 0 failed'

# Extract a field by name rather than substring-matching the whole line.
#
# The first version of these tests used [[ "$out" == *"failed=3"* ]]. That is the
# substring-assert defect in miniature: the emitted line also carries
# red_by_owner=kade=1;silas=2, so patterns like *"failed=1"* and bare counts
# collide across fields, and *"failed=3"* would equally match failed=30. Proven,
# not theorised — under a mutation that made the total count only kade's slice,
# the exact bug these tests exist to catch, the substring version stayed GREEN
# while the extracting version went red. Compare parsed values, never fragments.
field() { sed -n "s/.*[[:space:]]$2=\([^[:space:]]*\).*/\1/p" <<< "$1"; }

# --- the count is the board, not a slice -----------------------------------

@test "failed count is the BOARD total, not any single owner's slice" {
  out="$(run_summary_over "$MIXED")"
  # 3 red total: kade 1, silas 2. The 2026-08-04 defect reported a slice as the
  # total, so assert the number no per-owner nudge could produce.
  [ "$(field "$out" failed)" = "3" ]
}

@test "red_by_owner names every owner with a red, so no slice reads as the whole" {
  out="$(run_summary_over "$MIXED")"
  rbo="$(field "$out" red_by_owner)"
  [[ "$rbo" == *"kade=1"* ]]
  [[ "$rbo" == *"silas=2"* ]]
}

@test "skips are counted separately and never as failures (#3557)" {
  out="$(run_summary_over "$MIXED")"
  [ "$(field "$out" skipped)" = "1" ]
  [ "$(field "$out" failed)" = "3" ]
}

# --- NEGATIVE PROOFS (#3734) — the bar must be falsifiable both ways ---------

@test "NEGATIVE PROOF: a red board must NOT claim zero_red" {
  # The violated state: reds exist. If zero_red could still be true here, the
  # field would be decoration and the bar unmeasurable.
  out="$(run_summary_over "$MIXED")"
  [ "$(field "$out" zero_red)" = "false" ]
  
}

@test "NEGATIVE PROOF: zero_red=true is REACHABLE on a genuinely green board" {
  # The control. A field that is always false is as useless as one always true;
  # this is what makes the assertion above mean something.
  out="$(run_summary_over "$ALL_GREEN")"
  [ "$(field "$out" zero_red)" = "true" ]
  [ "$(field "$out" failed)" = "0" ]
}

@test "NEGATIVE PROOF: an empty results block does not fabricate a green board" {
  # A run that produced nothing (killed early, #3720) must not read as success.
  # zero_red is about MEASURED zero, not absent measurement.
  out="$(run_summary_over "")"
  [ "$(field "$out" suites)" = "0" ]
}

@test "the event is registered in spine-events.json (emit conformance)" {
  reg="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/designing/schemas/spine-events.json"
  grep -q "nightly.run.summary" "$reg"
}
