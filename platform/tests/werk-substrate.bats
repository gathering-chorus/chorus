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
  # #3721 — this used to just run `werk deploy` and expect a refusal, on the
  # assumption in its original comment: "we're on kade/2598-* while this card is
  # in flight, so HEAD will not match origin/main". That assumption was true the
  # day it was written and false every day since. A freshly-pulled werk has NO
  # commits yet, so its HEAD IS origin/main (verified: both 873769c4a) and the
  # guard correctly ALLOWS the deploy — and a CI checkout of main is the same.
  # The test was reading ambient git state instead of creating the condition it
  # claims to test, so it failed on a working guard.
  #
  # werk derives CHORUS_ROOT from its own location (script line 27), ignoring the
  # env, so the refusal cannot be driven from outside. Assert the guard STRUCTURALLY
  # instead — tier-appropriate for this file's declared "hermetic source guard",
  # and it catches the regression that actually matters: someone deleting the
  # check or unwiring it from the canonical deploy path.
  # Runtime refusal is exercised for real every time a role runs `werk deploy`
  # from a werk that has commits — which is the normal case mid-card.
  grep -q 'verify_main_sha()' "$WERK"
  grep -q 'rev-parse origin/main' "$WERK"
  grep -qi 'HEAD does not match origin/main' "$WERK"
  # ...and it must be WIRED into the canonical deploy path, not merely defined.
  run bash -c "sed -n '/^cmd_deploy()/,/^}/p' '$WERK' | grep -c verify_main_sha"
  [ "$output" -ge 1 ]
}
