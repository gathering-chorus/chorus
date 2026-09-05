#!/usr/bin/env bats
# @test-type: unit — hermetic: drives the footer function with a stub reconciler.
# No store, no network, no runner.
#
# #4015 — Jeff, 2026-08-26: "reports have headers and lines and data and at the
# end everything must cross reference — its not something u just throw away and
# start from scratch on every run."
#
# Measured against what exists: the header (chorus:TestSuiteRun, 636 rows), the
# lines (chorus:TestResult, 190,941 rows) and the footer (`werk-test reconcile`,
# main.rs:1421, which computes registered-minus-executed) are ALL built. The
# footer has zero callers — nothing in nightly-suites.sh, werk.yml, any plist or
# skill invokes --reconcile, and Loki shows 0 `tests.reconcile` events. So the
# ledger accumulates and is never cross-footed, and the per-run summary string
# becomes the only artifact.
#
# This wires the footer into the nightly as its own SUITE row. The rows below
# pin the three states that must stay separable, because a footer that cannot
# fail is worse than none: it certifies the ledger it never read.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"
  export NIGHTLY_RECONCILE_BIN="$TMP/werk-test"
}

# Stub the reconciler: $1 = what it prints, $2 = its exit code.
stub() {
  printf '#!/bin/bash\ncat <<"EOF"\n%s\nEOF\nexit %s\n' "$1" "${2:-0}" > "$TMP/werk-test"
  chmod +x "$TMP/werk-test"
}

leg() {
  bash -c "source '$NIGHTLY' --list-shell >/dev/null 2>&1; _reconcile_leg"
}

@test "control: a clean census reports pass and names the totals" {
  stub "reconcile: registered 7624, never-run: none"
  run leg
  [[ "$output" == "SUITE|reconcile|tests-domain|kade|pass|"* ]]
  [[ "$output" == *"7624"* ]]
}

@test "negative proof: registered tests that never ran are a FAIL, not a footnote" {
  # The state the footer exists to catch: the ledger says 7624 tests are
  # registered and the run executed fewer. Before #4015 nothing asked.
  stub "reconcile: registered 7624, never-run (3):
  platform/tests/a.bats :: does a thing
  platform/tests/b.bats :: does another
  platform/tests/c.bats :: third"
  run leg
  [[ "$output" == *"|fail|"* ]]
  [[ "$output" == *"3 registered test(s) never ran"* ]]
}

@test "negative proof: an unreachable ledger is UNMEASURED, never a pass" {
  # The 2026-08-26 shape: the tests domain 502s, the reconciler refuses, and a
  # footer that read nothing must not certify the run. This is the difference
  # between "cross-footed and clean" and "could not cross-foot".
  stub "reconcile requires the tests domain; fetch failed or empty" 1
  run leg
  [[ "$output" == *"|unmeasured|"* ]]
  [[ "$output" != *"|pass|"* ]]
}

@test "control: the unmeasured branch is reachable only on failure, not always" {
  # Without this the leg could report unmeasured unconditionally and still pass
  # the proof above — a footer that never certifies anything (#3734).
  stub "reconcile: registered 7624, never-run: none"
  run leg
  [[ "$output" != *"|unmeasured|"* ]]
}

@test "a missing reconciler binary is UNMEASURED and says so, never silently absent" {
  export NIGHTLY_RECONCILE_BIN="$TMP/does-not-exist"
  run leg
  [[ "$output" == *"|unmeasured|"* ]]
  [[ "$output" == *"reconciler not found"* ]]
}

@test "the run-all path actually calls the footer, so these proofs are not hollow" {
  # #4015's whole finding is that the footer existed and nothing called it.
  # Guard the wiring itself, or this file re-creates the defect it fixes.
  run bash -c "grep -c '_reconcile_leg' '$NIGHTLY'"
  [ "$output" -ge 2 ]
}

# ---- #4106 — the number carries its states, or says it cannot ----

@test "the row leads with the by-state split, not a bare count" {
  # Jeff, 2026-09-04: "i dont want tests that are like schrodinger's cat."
  # A count nobody can act on sat in the report every morning; the split is
  # what makes it actionable at a glance.
  stub "reconcile: registered 7934, never-run (209):
  by kind: sh 54, bats 49, ts 41
  by state: LANE SILENT 180, NAME MISMATCH 29
  platform/tests/a.bats :: does a thing"
  run leg
  [[ "$output" == *"|fail|"* ]]
  [[ "$output" == *"209 registered test(s) never ran"* ]]
  [[ "$output" == *"LANE SILENT 180, NAME MISMATCH 29"* ]]
  [[ "$output" != *"state UNKNOWN"* ]]
}

@test "negative proof: a census with no split says UNKNOWN, it does not drop it" {
  # The failure this catches: an older werk-test on the box prints no split,
  # the sed finds nothing, and the row renders exactly like a run that had no
  # states to report. Silence and absence must not look the same (#3734).
  stub "reconcile: registered 7934, never-run (209):
  by kind: sh 54, bats 49, ts 41
  platform/tests/a.bats :: does a thing"
  run leg
  [[ "$output" == *"|fail|"* ]]
  [[ "$output" == *"state UNKNOWN"* ]]
  [[ "$output" == *"predates #4106"* ]]
}

@test "negative proof: a clean census grows no split at all" {
  # Nothing never-ran, so no state may be reported for anything — the mirror
  # of the classifier's own "an empty gap fabricates no state".
  stub "reconcile: registered 7934, never-run: none"
  run leg
  [[ "$output" == *"|pass|"* ]]
  [[ "$output" != *"LANE SILENT"* ]]
  [[ "$output" != *"state UNKNOWN"* ]]
}
