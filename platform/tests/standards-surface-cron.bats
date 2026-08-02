#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# Tests for standards-surface-cron.sh (#2268)
# What Jeff sees: the standards surface updates itself overnight.
# These tests prove: source detection works, skip when unchanged, regen when changed.

SCRIPT="${CHORUS_ROOT}/platform/scripts/standards-surface-cron.sh"
GEN_SCRIPT="${CHORUS_ROOT}/platform/scripts/generate-standards-surface.sh"

@test "AC1: cron wrapper script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "AC2: dry-run detects source changes on first run (no prior checksums)" {
  # #3710 — this now does what the comment always claimed. It used to delete
  # /tmp/test-standards-checksums.json, a path the script never reads, then run
  # against the machine's REAL cached state — which correctly reported
  # "unchanged", so a first-run assertion could not pass on any box that had run
  # the cron before. STANDARDS_STATE_FILE is the seam.
  local state="${BATS_TEST_TMPDIR:-/tmp}/first-run-checksums-$$.json"
  rm -f "$state"
  run env STANDARDS_STATE_FILE="$state" bash "$SCRIPT" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"first run"* ]] || [[ "$output" == *"would regenerate"* ]]
}

# #3725 AC6 — these two declared "@test-type: unit — hermetic source guard" and
# were neither. Both ran `--force` with no seam set, so they wrote the REAL
# ~/.chorus/standards-surface-checksums.json and regenerated the REAL standards
# HTML into the repo from whatever sources the running machine could see. That is
# where hollow chorus-standards.html rebuilds (0 feedback rules, 0 stories) kept
# coming from. #3710 fixed exactly this in the sibling AC2 dry-run test above and
# these two were never migrated — the pattern existed, nothing made them adopt it.
#
# Both seams are now used: STANDARDS_STATE_FILE (#3710) for the checksum state,
# STANDARDS_OUTPUT_DIR (#3725) for the generated output. The tests assert against
# their own paths, so they finally test what they claim without touching the
# machine they run on.

@test "AC2: skip when sources unchanged — checksum file persists after force run" {
  local state="${BATS_TEST_TMPDIR}/checksums-$$.json"
  local outdir="${BATS_TEST_TMPDIR}/out-$$"
  mkdir -p "$outdir"
  rm -f "$state"
  env STANDARDS_STATE_FILE="$state" STANDARDS_OUTPUT_DIR="$outdir" \
    bash "$SCRIPT" --force 2>/dev/null || true
  # The state file the RUN was told to use is the one that must exist.
  [ -f "$state" ]
  [[ "$(cat "$state")" == *"decisions"* ]]
}

@test "AC1: force flag always regenerates" {
  local state="${BATS_TEST_TMPDIR}/force-checksums-$$.json"
  local outdir="${BATS_TEST_TMPDIR}/force-out-$$"
  mkdir -p "$outdir"
  run env STANDARDS_STATE_FILE="$state" STANDARDS_OUTPUT_DIR="$outdir" \
    bash "$SCRIPT" --force
  [ "$status" -eq 0 ]
  [[ "$output" == *"Forced regeneration"* ]]
  [[ "$output" == *"complete"* ]]
}

@test "AC3: generation script exists (dependency)" {
  [ -x "$GEN_SCRIPT" ]
}
