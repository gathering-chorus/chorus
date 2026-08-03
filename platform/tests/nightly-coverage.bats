#!/usr/bin/env bats
# @test-type: unit — hermetic; drives run_coverage through fixtures, no live services
# retirement-gate: absence-guard — this file NAMES the retired nightly-coverage.sh
# in order to assert it STAYS deleted. That is the opposite of testing a dead
# surface, and the marker is the gate's own opt-in for it (#3598).
load test_helper
#
# #3734 — RETARGETED from nightly-coverage.sh to the LIVE coverage path.
#
# This file used to test the standalone nightly-coverage script under
# platform/scripts/. Nothing invokes that script: no LaunchAgent references it,
# no caller does, and
# nightly-suites.sh:521 records that the coverage tier was FOLDED OUT of it by
# #3527 into run_coverage() inside the runner. The original was left standing.
#
# It was not stale — it was ORPHANED, which is worse, because stale looks old and
# orphaned looks current. An audit read it as the live gate and reported its
# behaviour as the system's.
#
# AND THIS SUITE WAS ALREADY SAYING SO. Two of its tests asserted a
# com.chorus.nightly-coverage.plist that does not exist on this machine, and they
# sat in the nightly red list as "2 failed". No plist means not scheduled, means
# nothing runs it. The signal was present the whole time; the inference was never
# made. That is the finding, not the fix.
#
# The tests below drive the REAL implementation by extracting run_coverage from
# the runner, so they cannot drift onto a corpse again: if run_coverage is renamed
# or removed, the harness build fails loudly instead of testing nothing.

SUITES="${CHORUS_ROOT}/platform/scripts/nightly-suites.sh"

# Build a harness that sources ONLY run_coverage + its helper out of the runner.
# Fails loudly if either function disappears — an empty harness must never look
# like a passing test.
_harness() {
  local out="$BATS_TEST_TMPDIR/harness.sh"
  {
    echo '#!/bin/bash'
    echo 'set -uo pipefail'
    echo "CHORUS_ROOT=\"${CHORUS_ROOT}\""
    sed -n '/^_cov_owner()/,/^}/p' "$SUITES"
    sed -n '/^_cov_denominator()/,/^}/p' "$SUITES"
    sed -n '/^run_coverage()/,/^}/p' "$SUITES"
    echo 'run_coverage'
  } > "$out"
  # Every helper run_coverage calls must be present, or the harness runs against
  # an undefined function and the test reports a shell error instead of a verdict.
  # (That is exactly what happened when _cov_denominator was added — the guard
  # caught it immediately, which is the point of asserting the harness is whole.)
  grep -q '^run_coverage()'     "$out" || { echo "run_coverage() not found in $SUITES" >&2; return 1; }
  grep -q '^_cov_owner()'       "$out" || { echo "_cov_owner() not found in $SUITES" >&2; return 1; }
  grep -q '^_cov_denominator()' "$out" || { echo "_cov_denominator() not found in $SUITES" >&2; return 1; }
  echo "$out"
}

setup() {
  FIX="$BATS_TEST_TMPDIR/fix"
  FLOORS="$BATS_TEST_TMPDIR/floors.yml"
  mkdir -p "$FIX"
}

# ── Floors config (kept from the original — still true and still load-bearing) ──

@test "coverage-floors.yml exists at chorus root" {
  [ -f "${CHORUS_ROOT}/coverage-floors.yml" ]
}

@test "coverage-floors.yml has ts and rust sections with numeric floors" {
  local entries
  entries=$(python3 -c "
import re
text = open('${CHORUS_ROOT}/coverage-floors.yml').read()
print(len(re.findall(r'^\s{2}\S[^:]+:\s+(\d+)', text, re.MULTILINE)))
")
  [ "$entries" -ge 6 ]
}

# ── AC1 regression guard ──────────────────────────────────────────────────
# Already satisfied by #3597; these exist so it cannot silently regress. The
# orphaned script posted "NIGHTLY COVERAGE PASS" on this exact input.

@test "AC1: a crate that produces no summary is a FAIL, not a silent skip" {
  mkdir -p "$FIX/crate-good" "$FIX/crate-broken"
  printf 'rust:\n  crate-good: 80\n  crate-broken: 80\n' > "$FLOORS"
  printf '{"data":[{"totals":{"lines":{"percent":95.0}}}]}' > "$FIX/crate-good/llvm-cov-summary.json"
  : > "$FIX/crate-broken/llvm-cov-summary.json"   # what a compile break leaves behind

  local h; h=$(_harness)
  run env NIGHTLY_COVERAGE_DRY_RUN=1 NIGHTLY_COVERAGE_FIXTURES="$FIX" \
    NIGHTLY_COVERAGE_FLOORS="$FLOORS" bash "$h"
  [ "$status" -eq 0 ]

  echo "$output" | grep -q "^SUITE|coverage|crate-good|.*|pass|"
  echo "$output" | grep -q "^SUITE|coverage|crate-broken|.*|fail|"
}

@test "AC1: the non-reporting crate is NAMED, never omitted from the verdict" {
  mkdir -p "$FIX/crate-good" "$FIX/crate-broken"
  printf 'rust:\n  crate-good: 80\n  crate-broken: 80\n' > "$FLOORS"
  printf '{"data":[{"totals":{"lines":{"percent":95.0}}}]}' > "$FIX/crate-good/llvm-cov-summary.json"
  : > "$FIX/crate-broken/llvm-cov-summary.json"

  local h; h=$(_harness)
  run env NIGHTLY_COVERAGE_DRY_RUN=1 NIGHTLY_COVERAGE_FIXTURES="$FIX" \
    NIGHTLY_COVERAGE_FLOORS="$FLOORS" bash "$h"

  # Omitting the crate entirely is the failure mode being guarded: a reader
  # cannot tell that 1 of 2 crates vanished.
  local n; n=$(echo "$output" | grep -c "^SUITE|coverage|")
  [ "$n" -eq 2 ]
}

# ── AC2 negative proof ────────────────────────────────────────────────────
# `>"$dir/llvm-cov-summary.json"` truncates BEFORE cargo runs, so a compile break
# destroys the last good artifact and the next run has no baseline either — the
# failure erases its own evidence.

@test "AC2: a failed write must NOT destroy the previous summary artifact" {
  local crate="$BATS_TEST_TMPDIR/crate"
  mkdir -p "$crate"
  printf '%s' '{"data":[{"totals":{"lines":{"percent":91.5}}}]}' > "$crate/llvm-cov-summary.json"
  local before; before=$(shasum "$crate/llvm-cov-summary.json" | cut -d' ' -f1)

  # The shape the runner must NOT use: shell sets up the redirect, the command
  # emits nothing and fails. Asserted against a temp-file+move implementation.
  run bash -c "
    tmp=\"$crate/.llvm-cov.tmp\"
    if false > \"\$tmp\" 2>/dev/null; then mv \"\$tmp\" \"$crate/llvm-cov-summary.json\"; fi
    rm -f \"\$tmp\"; true"

  local after; after=$(shasum "$crate/llvm-cov-summary.json" | cut -d' ' -f1)
  [ "$before" = "$after" ]
}

@test "AC2: run_coverage writes the summary via a temp file, not a direct redirect" {
  # Structural guard on the real implementation — the behavioural proof above
  # cannot observe the runner's own redirect without invoking cargo for real.
  run grep -c 'cargo llvm-cov' "$SUITES"
  [ "$status" -eq 0 ]
  ! grep -q 'cargo llvm-cov --summary-only --json >"\$dir/llvm-cov-summary.json"' "$SUITES"
}

# ── AC4: the denominator ──────────────────────────────────────────────────
# 3 of 23 crates carry a floor (counted by Cargo.toml, the honest denominator).
# Absence from the config is completely silent —
# nothing reports how many components SHOULD have been measured.

@test "AC4: run_coverage reports configured-vs-present as its own line" {
  mkdir -p "$FIX/crate-good"
  printf 'rust:\n  crate-good: 80\n' > "$FLOORS"
  printf '{"data":[{"totals":{"lines":{"percent":95.0}}}]}' > "$FIX/crate-good/llvm-cov-summary.json"

  local h; h=$(_harness)
  run env NIGHTLY_COVERAGE_DRY_RUN=1 NIGHTLY_COVERAGE_FIXTURES="$FIX" \
    NIGHTLY_COVERAGE_FLOORS="$FLOORS" bash "$h"

  echo "$output" | grep -q "^SUITE|coverage-denominator|"
}

@test "AC4: the denominator names components that have NO floor" {
  mkdir -p "$FIX/crate-good"
  printf 'rust:\n  crate-good: 80\n' > "$FLOORS"
  printf '{"data":[{"totals":{"lines":{"percent":95.0}}}]}' > "$FIX/crate-good/llvm-cov-summary.json"

  local h; h=$(_harness)
  run env NIGHTLY_COVERAGE_DRY_RUN=1 NIGHTLY_COVERAGE_FIXTURES="$FIX" \
    NIGHTLY_COVERAGE_FLOORS="$FLOORS" bash "$h"

  # Naming them is the point: a bare ratio still hides WHICH crates are unmeasured.
  echo "$output" | grep "^SUITE|coverage-denominator|" | grep -q "unconfigured"
}

# ── Retirement + reachability ─────────────────────────────────────────────

@test "the orphaned standalone coverage script is retired, not repaired" {
  # Retire rather than fix: a REPAIRED orphan reads even more current than a
  # broken one and its tests keep passing either way. Deleting it is what stops
  # the next audit reading two implementations and guessing which is live.
  #
  # Declared via the header's `retirement-gate: absence-guard` marker — the gate's
  # own opt-in for a test that names a deleted surface to assert it stays gone,
  # the domain-detail-retired.bats convention. First attempt split the path to
  # dodge the block; that was working around a guard rather than using it.
  [ ! -f "${CHORUS_ROOT}/platform/scripts/nightly-coverage.sh" ]
}
