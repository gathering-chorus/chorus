#!/usr/bin/env bats
# @test-type: integration:security — runs the real semgrep ruleset over seeded fixtures; hermetic (no network, no live service)
# @domain: security — the product domain this suite guards (#4334)
# #3981 — the security-scan spike must be PROVEN to catch a real finding, not
# just run (#3734). The seeded fixtures ARE the proof: the vuln fixture carries
# the exact #3980 caller-supplied-`from` pattern and must FAIL the scan; the
# safe (fixed) fixture must PASS. A scanner that can't tell them apart is worthless.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/test-security-scan.sh"
# Covers: platform/scripts/test-security-scan.sh  (literal path for the #3917 linker)
FIX="${BATS_TEST_DIRNAME}/../security/semgrep/fixtures"

# #4925 (Silas) — semgrep lives in ~/.local/bin, which launchd's PATH (the nightly's)
# does not carry, so all three cases skipped every night. Find it there too, put its
# directory on PATH for the scan script, and under the nightly a missing semgrep is RED
# (named), never a skip. Off the nightly a box without semgrep still skips.
SEMGREP="${SEMGREP:-$(command -v semgrep 2>/dev/null || echo "$HOME/.local/bin/semgrep")}"

setup() {
  if [ ! -x "$SEMGREP" ]; then
    [ "${WERK_TEST_NIGHTLY:-}" = 1 ] && { echo "semgrep not found at $SEMGREP"; return 1; }
    skip "semgrep not installed"
  fi
  PATH="$(dirname "$SEMGREP"):$PATH"
}

@test "NEGATIVE PROOF (#4925): under the nightly a missing semgrep is red, not a skip" {
  run env SEMGREP=/nonexistent/semgrep WERK_TEST_NIGHTLY=1 bats -f "PASSES the scan" "$BATS_TEST_FILENAME"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"semgrep not found at /nonexistent/semgrep"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"# skip"* ]] || { echo "skipped: $output"; return 1; }
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
