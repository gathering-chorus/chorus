#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# @domain: policies — the product domain this suite guards (#4334)
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
  [[ "$(cat "$state")" == *"decisions"* ]] || return 1
}

@test "AC1: force flag always regenerates" {
  local state="${BATS_TEST_TMPDIR}/force-checksums-$$.json"
  local outdir="${BATS_TEST_TMPDIR}/force-out-$$"
  mkdir -p "$outdir"
  run env STANDARDS_STATE_FILE="$state" STANDARDS_OUTPUT_DIR="$outdir" \
    bash "$SCRIPT" --force
  [ "$status" -eq 0 ]
  [[ "$output" == *"Forced regeneration"* ]] || return 1
  [[ "$output" == *"complete"* ]] || return 1
}

@test "AC3: generation script exists (dependency)" {
  [ -x "$GEN_SCRIPT" ]
}

# #4722 — with no STANDARDS_OUTPUT_DIR the arg array is empty, and macOS's
# /bin/bash 3.2 treats "${a[@]}" of an empty array as unbound under set -u: the
# job exited 1 every day from 2026-10-23 05:30. Hermetic: the script runs from a
# copy beside a stub generator, against an empty CHORUS_ROOT, on the system bash.
@test "#4722: a forced run with no output dir completes on bash 3.2 and calls the generator with no args" {
  local dir="${BATS_TEST_TMPDIR}/cron-4722" root="${BATS_TEST_TMPDIR}/root-4722"
  mkdir -p "$dir/lib" "$root"
  cp "$SCRIPT" "$dir/standards-surface-cron.sh"
  cp "$(dirname "$SCRIPT")/lib/service-lifecycle.sh" "$dir/lib/"
  printf '#!/bin/bash\necho "args=$#" > "%s/gen-called"\n' "$dir" > "$dir/generate-standards-surface.sh"
  chmod +x "$dir/generate-standards-surface.sh"
  run env -u STANDARDS_OUTPUT_DIR CHORUS_ROOT="$root" STANDARDS_STATE_FILE="$dir/state.json" \
    /bin/bash "$dir/standards-surface-cron.sh" --force
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$dir/gen-called")" = "args=0" ]
}
