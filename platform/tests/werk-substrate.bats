#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# werk-substrate.bats — #2598 substrate uniformity
# What Jeff sees: all three roles execute the same way for build/deploy/check.
# These tests cover the werk wrapper. (#3290: the pre-push hook tests were
# removed — platform/hooks/pre-push was retired with git-queue.sh #3182/#3223;
# branch + role push validation now lives in the werk-push binary and is
# covered by platform/services/werk-push/tests/e2e.rs.)

WERK="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}/platform/scripts/werk"
[ -x "$WERK" ] || WERK="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/werk"

# --- werk check ---

@test "werk check exits 0 and emits drift report" {
  run bash "$WERK" check
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "drift" || (echo "expected 'drift' in output: $output" && false)
  echo "$output" | grep -q "git HEAD" || (echo "expected git state in output: $output" && false)
}

@test "werk check is read-only (no files modified)" {
  # Snapshot mtime of canonical binary if it exists
  local shim="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  if [ -f "$shim" ]; then
    local before_mtime
    before_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
    run bash "$WERK" check
    local after_mtime
    after_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
    [ "$before_mtime" = "$after_mtime" ] || (echo "werk check mutated the binary mtime" && false)
  fi
}

@test "werk help shows substrate framing" {
  run bash "$WERK" help
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "execute work-units against the chorus substrate"
}

# --- werk deploy refusal (no main checkout) ---

@test "werk deploy refuses when HEAD != origin/main" {
  # We're on kade/2598-* branch by definition while this card is in flight,
  # so HEAD will not match origin/main. Verify werk deploy refuses.
  run bash "$WERK" deploy
  [ "$status" -ne 0 ]
  echo "$output" | grep -qi "main\|HEAD" || (echo "expected main/HEAD diagnostic, got: $output" && false)
}
