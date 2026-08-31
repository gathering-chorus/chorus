#!/usr/bin/env bats
# @test-type: integration — runs real trivy over seeded lockfile fixtures; no live service.
# #4034 — the nightly SCA scan is SCOPED (skip target/, node_modules/, .git)
# because the unscoped 17GB walk ran 72+ min under contention and blew the
# lane cap (2026-08-30). Negative proofs (#3734), both directions:
#   1. the scoped flags still FAIL on a planted HIGH CVE — faster, not blinder;
#   2. the same CVE hidden inside a target/ dir is invisible scoped and
#      VISIBLE deep — the skip provably skips, and the weekly deep covers it.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/test-security-scan.sh"
# Covers: platform/scripts/test-security-scan.sh  (literal path for the #3917 linker)
FIX="${BATS_TEST_DIRNAME}/../security/sca-fixtures/vulnerable"
ART="${BATS_TEST_DIRNAME}/../security/sca-fixtures/artifact-only"

setup() {
  command -v trivy >/dev/null 2>&1 || skip "trivy not installed"
}

@test "NEGATIVE PROOF: scoped scan still fails on the planted HIGH CVE (lodash 4.17.15)" {
  run bash "$SCRIPT" sca-selftest "$FIX"
  [ "$status" -ne 0 ]
  [[ "$output" == *"lodash"* ]]
}

@test "the skip provably skips: the same lockfile under target/ is invisible scoped, visible deep" {
  run bash "$SCRIPT" sca-selftest "$ART"
  local scoped="$status"
  run bash "$SCRIPT" sca-selftest-deep "$ART"
  local deep="$status"
  [ "$scoped" -eq 0 ]
  [ "$deep" -ne 0 ]
}

@test "the repo scan excludes the fixture dir itself — planted CVEs never red the nightly" {
  run grep -q 'sca-fixtures' "$SCRIPT"
  [ "$status" -eq 0 ]
}
