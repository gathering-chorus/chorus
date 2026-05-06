#!/usr/bin/env bash
# test-chorus-inject-deployed.sh — regression guard for #2769.
#
# CLAUDE.md and #2734 both say chorus-inject deploys to ~/.chorus/bin/
# alongside chorus-hook-shim and chorus-hooks. Until #2769, the actual
# deploy step had never been run — the binary lived only at
# target/release/chorus-inject, where the cdhash churns on every cargo
# build and TCC silently revokes AppleEvents permission.
#
# This test asserts the deploy is in place. If it fails after a future
# change, run: bash platform/scripts/build-signed.sh chorus-inject
#
# Run directly (not via Claude hook-intercepted Bash).
set -uo pipefail

BIN="$HOME/.chorus/bin/chorus-inject"
PASS=0
FAIL=0

assert() {
  local label="$1"; shift
  if "$@"; then
    PASS=$((PASS + 1))
    echo "PASS: $label"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL: $label"
  fi
}

assert "chorus-inject exists at ~/.chorus/bin/" test -x "$BIN"
assert "chorus-inject is executable" test -f "$BIN"

# Verify it's signed by the Chorus Local Signing identity (stable cdhash
# across rebuilds is the whole point — ad-hoc-signed binaries don't
# survive TCC AppleEvents grant after rebuild).
if [ -x "$BIN" ]; then
  AUTHORITY=$(codesign -dvvv "$BIN" 2>&1 | grep "^Authority=" | head -1)
  assert "chorus-inject signed by Chorus Local Signing" \
    grep -q "Chorus Local Signing" <<< "$AUTHORITY"

  IDENTIFIER=$(codesign -dvvv "$BIN" 2>&1 | grep "^Identifier=" | head -1)
  assert "chorus-inject identifier=com.chorus.inject" \
    grep -q "com.chorus.inject" <<< "$IDENTIFIER"

  # Smoke: the binary actually runs and prints its usage banner. If the
  # deploy artifact is corrupt or wrong-arch, this catches it.
  USAGE=$("$BIN" 2>&1 || true)
  assert "chorus-inject prints Usage banner when called with no args" \
    grep -q "Usage:" <<< "$USAGE"
fi

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
