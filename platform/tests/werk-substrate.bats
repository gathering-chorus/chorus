#!/usr/bin/env bats
# @test-type: unit — runs werk; the deploy gate runs in a throwaway git root
# @domain: pipelines — the product domain this suite guards (#4334)
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
  # Snapshot mtime of canonical binary. #4336: when it is absent there is
  # nothing to watch, and a case that asserts nothing must not read as a pass.
  local shim="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  [ -f "$shim" ] || skip "UNMEASURED — chorus-hook-shim not built in this tree, nothing to watch (#4336)"
  local before_mtime
  before_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
  run bash "$WERK" check
  local after_mtime
  after_mtime=$(stat -f '%m' "$shim" 2>/dev/null || stat -c '%Y' "$shim" 2>/dev/null)
  [ "$before_mtime" = "$after_mtime" ] || (echo "werk check mutated the binary mtime" && false)
}

@test "werk help shows substrate framing" {
  run bash "$WERK" help
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "execute work-units against the chorus substrate"
}

# --- werk deploy refusal (no main checkout) ---

# #4336 — werk derives CHORUS_ROOT from its own location, so the gate is
# driven by giving it a root of our own: a throwaway git repo holding a copy of
# werk, a chorus-log stub that records spine events, and a build-signed.sh stub
# that records being called and then stops the deploy (exit 1), so a deploy the
# gate wrongly lets through goes no further than the recorder — nothing builds,
# nothing is written outside $BATS_TEST_TMPDIR.
fake_root() {
  R="$BATS_TEST_TMPDIR/root"
  mkdir -p "$R/platform/scripts"
  cp "$WERK" "$R/platform/scripts/werk"
  printf '#!/bin/bash\necho "$*" >> "%s/spine.log"\n' "$BATS_TEST_TMPDIR" > "$R/platform/scripts/chorus-log"
  printf '#!/bin/bash\necho "build-signed $*" >> "%s/build.log"\nexit 1\n' "$BATS_TEST_TMPDIR" > "$R/platform/scripts/build-signed.sh"
  chmod +x "$R/platform/scripts/chorus-log" "$R/platform/scripts/build-signed.sh"
  g() { git -C "$R" -c user.email=t@t -c user.name=t -c commit.gpgsign=false "$@"; }
  g init -q && g add -A && g commit -qm base && g update-ref refs/remotes/origin/main HEAD
}

@test "werk deploy refuses when HEAD != origin/main" {
  # #3721 — this used to read ambient git state (a fresh werk's HEAD IS
  # origin/main), then became a grep of the script. #4336: create the
  # condition and run the refusal.
  fake_root
  echo change > "$R/change.txt"
  g add -A && g commit -qm ahead
  run env DEPLOY_ROLE=kade bash "$R/platform/scripts/werk" deploy
  [ "$status" -eq 1 ]
  [[ "$output" == *"HEAD does not match origin/main"* ]] || return 1
  [[ "$output" != *"=== werk deploy"* ]] || return 1
  # refused before any build step, and the refusal is on the spine
  [ ! -e "$BATS_TEST_TMPDIR/build.log" ]
  grep -q "^werk.deploy.refused kade reason=non-main-sha mode=canonical" "$BATS_TEST_TMPDIR/spine.log"
}

@test "werk deploy on HEAD == origin/main passes the gate (the refusal is not unconditional)" {
  # The other state the gate must separate: same root, no extra commit. The
  # gate lets it through to the (stubbed, stopping) build step.
  fake_root
  run env DEPLOY_ROLE=kade bash "$R/platform/scripts/werk" deploy
  [[ "$output" != *"HEAD does not match origin/main"* ]] || return 1
  [[ "$output" == *"=== werk deploy (canonical) ==="* ]] || return 1
  grep -q "^build-signed chorus-hooks" "$BATS_TEST_TMPDIR/build.log"
  run grep -q "werk.deploy.refused" "$BATS_TEST_TMPDIR/spine.log"
  [ "$status" -ne 0 ]
}
