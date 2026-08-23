#!/usr/bin/env bats
# @test-type: integration — runs the real semgrep ruleset over seeded fixtures; hermetic (no network, no live service)
# #3981 — the security-scan spike must be PROVEN to catch a real finding, not
# just run (#3734). The seeded fixtures ARE the proof: the vuln fixture carries
# the exact #3980 caller-supplied-`from` pattern and must FAIL the scan; the
# safe (fixed) fixture must PASS. A scanner that can't tell them apart is worthless.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/test-security-scan.sh"
# Covers: platform/scripts/test-security-scan.sh  (literal path for the #3917 linker)
FIX="${BATS_TEST_DIRNAME}/../security/semgrep/fixtures"

setup() {
  command -v semgrep >/dev/null 2>&1 || skip "semgrep not installed"
}

@test "NEGATIVE PROOF: the seeded #3980-pattern fixture FAILS the scan" {
  run bash "$SCRIPT" selftest "$FIX/vuln-from-spoofing.ts"
  [ "$status" -ne 0 ]
}

@test "the fixed (identity-bound) fixture PASSES the scan" {
  run bash "$SCRIPT" selftest "$FIX/safe-from-binding.ts"
  [ "$status" -eq 0 ]
}

@test "the rule separates violation from fix — proof both directions in one run" {
  run bash "$SCRIPT" selftest "$FIX/vuln-from-spoofing.ts"
  local vuln="$status"
  run bash "$SCRIPT" selftest "$FIX/safe-from-binding.ts"
  local safe="$status"
  [ "$vuln" -ne 0 ]
  [ "$safe" -eq 0 ]
}
