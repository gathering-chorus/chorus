#!/usr/bin/env bats
# @test-type: unit — sources the harness's summary extractor; no suites executed
# #4004 — the nightly must tell "correctly refused" from "failed". Kade's ask:
# pin BOTH states so the next harness change cannot re-merge them.
#
# Live case: test-product-membrane exits 3 by design when it detects a chorus
# agent ancestor (#3722 self-guard). The synthesizer scored every nonzero rc as
# "0 pass, 1 fail", so a guard behaving correctly sat red in the nightly for
# weeks while never running — a verdict that cannot separate the two states it
# exists to separate (#3734).

setup() {
  NS="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  # source just the extractor: the file guards its main on invocation args
  extract() { NIGHTLY_SOURCE_ONLY=1 bash -c '
    source "$1" >/dev/null 2>&1 || true
    _extract_shell_summary "$2" "$3"' _ "$NS" "$2" "$3"; }
}

@test "SELF-REFUSAL: rc=3 with no parseable line reports as refused, never as a failure" {
  run extract "" "REFUSED — runs under a chorus agent ancestor" 3
  [[ "$output" == *"SELF-REFUSED"* ]]
  [[ "$output" != *"1 fail"* ]]
}

@test "NEGATIVE PROOF: a real failure (rc=1) is still RED (#3734)" {
  run extract "" "boom" 1
  [[ "$output" == *"0 pass, 1 fail"* ]]
  [[ "$output" != *"SELF-REFUSED"* ]]
}

@test "a clean run (rc=0) is still green" {
  run extract "" "all good" 0
  [[ "$output" == *"1 ok, 0 fail"* ]]
}

@test "refusal reports ZERO failures — it must not read as green either" {
  run extract "" "REFUSED" 3
  [[ "$output" == *"0 pass, 0 fail"* ]]
}

@test "destructive-suite exclusion matches by BASENAME, so werk-tree copies are excluded too" {
  grep -q 'basename' "$NS"
  # the literal-path grep -vFf form is what let chorus-werk copies through
  ! grep -q 'grep -vFf <(printf .%s..n. \$NIGHTLY_DESTRUCTIVE_SUITES)' "$NS"
}

# --- #4004 (2): per-suite failure logs must be per-SUITE ----------------------
# Wren, 2026-08-25: all 14 logs from the 05:06 run were byte-identical
# (155245 bytes, same second) — one werk-test lane blob copied into every
# failing suite's name, so the logs could not answer the only question they
# exist for: WHICH suite failed.

@test "failure logs are per-unit slices, not N copies of the lane blob" {
  NS="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  # the writer must reference a single shared blob and grep the unit's own lines
  grep -q '_lane-output.log' "$NS"
  grep -q 'grep -F -- "\$unit"' "$NS"
}

@test "NEGATIVE PROOF: the old whole-blob write is gone (#3734)" {
  NS="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  # the exact line that produced 14 identical files
  ! grep -qE '^\s+printf .%s.n. "\$out" > "\$_flog"' "$NS"
}

# --- #4004 (3): coverage floors name crates that EXIST ------------------------
# owl-api was renamed athena-make (#3561). The floor kept pointing at the old
# name, so the lane measured a husk (target/ only, no source) and errored
# rc=101 every night — a red that could never go green.

@test "every rust coverage floor names a crate with source in the tree" {
  FLOORS="$BATS_TEST_DIRNAME/../../coverage-floors.yml"
  miss=""
  while read -r rel; do
    [ -d "$BATS_TEST_DIRNAME/../../$rel/src" ] || miss="$miss $rel"
  done < <(grep -oE '^  platform/services/[a-z0-9-]+' "$FLOORS" | awk '{print $1}')
  [ -z "$miss" ] || { echo "floors naming crates with no src/:$miss"; false; }
}

@test "NEGATIVE PROOF: the renamed-away owl-api floor is gone (#3734)" {
  FLOORS="$BATS_TEST_DIRNAME/../../coverage-floors.yml"
  ! grep -qE '^  platform/services/owl-api:' "$FLOORS"
  grep -qE '^  platform/services/athena-make:' "$FLOORS"
}
