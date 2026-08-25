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
