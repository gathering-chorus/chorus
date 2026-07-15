#!/usr/bin/env bats
# @test-type: unit
# #3628 — chorus-sdk-deploy.sh shape: build-verify-swap must be atomic.
# A failed or incomplete build leaves the live dist untouched; a good build
# swaps in atomically with the previous build kept at dist.prev.
# The test brings its own world: fixture package + CHORUS_SDK_BUILD_CMD stub.

REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/chorus-sdk-deploy.sh"

setup() {
  SDK="$BATS_TEST_TMPDIR/sdk"
  mkdir -p "$SDK/src" "$SDK/dist"
  echo "// ts" > "$SDK/src/emit.ts"
  echo "// ts" > "$SDK/src/token.ts"
  echo "old" > "$SDK/dist/emit.js"
}

@test "deploy script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "good build swaps dist atomically and keeps dist.prev" {
  run env CHORUS_SDK_DIR="$SDK" CHORUS_LOG_BIN=/usr/bin/true \
    CHORUS_SDK_BUILD_CMD="$REPO_ROOT/platform/tests/fixtures/3628-stub-build-good.sh" "$SCRIPT"
  [ "$status" -eq 0 ]
  [ -f "$SDK/dist/token.js" ]
  [ -f "$SDK/dist.prev/emit.js" ]
  [ "$(cat "$SDK/dist.prev/emit.js")" = "old" ]
}

@test "failed build leaves live dist untouched" {
  run env CHORUS_SDK_DIR="$SDK" CHORUS_LOG_BIN=/usr/bin/true \
    CHORUS_SDK_BUILD_CMD=/usr/bin/false "$SCRIPT"
  [ "$status" -eq 1 ]
  [ "$(cat "$SDK/dist/emit.js")" = "old" ]
  [ ! -d "$SDK/dist.prev" ]
}

@test "build missing a src counterpart refuses the swap (the token.js miss)" {
  run env CHORUS_SDK_DIR="$SDK" CHORUS_LOG_BIN=/usr/bin/true \
    CHORUS_SDK_BUILD_CMD="$REPO_ROOT/platform/tests/fixtures/3628-stub-build-partial.sh" "$SCRIPT"
  [ "$status" -eq 1 ]
  [[ "$output" == *"token"* ]]
  [ "$(cat "$SDK/dist/emit.js")" = "old" ]
}
