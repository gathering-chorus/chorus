#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# chorus-inject-signed-stable.bats — #2548 AC1.
#
# Asserts that build-signed.sh produces a chorus-inject binary whose cdhash
# is stable across rebuilds. macOS TCC binds AppleEvents permission to the
# cdhash; if rebuilds churn the cdhash, every build silently revokes the
# grant and nudge delivery breaks. Stable cdhash → grant survives → the
# nudge transport stops being intermittent.
#
# Same gate also asserts chorus-hook-shim's signed build (sibling script).

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
INJECT_DIR="$CHORUS_ROOT/platform/services/chorus-inject"
HOOKS_DIR="$CHORUS_ROOT/platform/services/chorus-hooks"
BUILD_SIGNED="$CHORUS_ROOT/platform/scripts/build-signed.sh"

cdhash_of() {
  codesign -dvvv "$1" 2>&1 | grep "^CDHash=" | head -1 | sed 's/^CDHash=//'
}

identifier_of() {
  codesign -dvvv "$1" 2>&1 | grep "^Identifier=" | head -1 | sed 's/^Identifier=//'
}

authority_of() {
  codesign -dvvv "$1" 2>&1 | grep "^Authority=" | head -1 | sed 's/^Authority=//'
}

@test "central build-signed.sh exists and is executable" {
  [ -x "$BUILD_SIGNED" ]
}

@test "build-signed.sh chorus-inject pins identifier=com.chorus.inject" {
  bash "$BUILD_SIGNED" chorus-inject >/dev/null 2>&1
  [ "$(identifier_of "$INJECT_DIR/target/release/chorus-inject")" = "com.chorus.inject" ]
}

@test "build-signed.sh chorus-inject signs with keychain identity (not ad-hoc)" {
  bash "$BUILD_SIGNED" chorus-inject >/dev/null 2>&1
  AUTH=$(authority_of "$INJECT_DIR/target/release/chorus-inject")
  [ -n "$AUTH" ]
  [ "$AUTH" != "Ad-hoc" ]
}

@test "build-signed.sh chorus-inject cdhash is stable across two consecutive runs" {
  bash "$BUILD_SIGNED" chorus-inject >/dev/null 2>&1
  HASH1=$(cdhash_of "$INJECT_DIR/target/release/chorus-inject")
  bash "$BUILD_SIGNED" chorus-inject >/dev/null 2>&1
  HASH2=$(cdhash_of "$INJECT_DIR/target/release/chorus-inject")
  [ -n "$HASH1" ]
  [ "$HASH1" = "$HASH2" ]
}

@test "build-signed.sh chorus-hooks pins identifier=com.chorus.hook-shim" {
  bash "$BUILD_SIGNED" chorus-hooks >/dev/null 2>&1
  [ "$(identifier_of "$HOOKS_DIR/target/release/chorus-hook-shim")" = "com.chorus.hook-shim" ]
}

@test "build-signed.sh chorus-hooks also signs chorus-hooks bin as com.chorus.hooks" {
  bash "$BUILD_SIGNED" chorus-hooks >/dev/null 2>&1
  [ "$(identifier_of "$HOOKS_DIR/target/release/chorus-hooks")" = "com.chorus.hooks" ]
}

@test "build-signed.sh chorus-hooks cdhash stable across consecutive runs" {
  bash "$BUILD_SIGNED" chorus-hooks >/dev/null 2>&1
  HASH1=$(cdhash_of "$HOOKS_DIR/target/release/chorus-hook-shim")
  bash "$BUILD_SIGNED" chorus-hooks >/dev/null 2>&1
  HASH2=$(cdhash_of "$HOOKS_DIR/target/release/chorus-hook-shim")
  [ -n "$HASH1" ]
  [ "$HASH1" = "$HASH2" ]
}
