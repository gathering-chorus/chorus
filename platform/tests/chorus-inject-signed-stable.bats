#!/usr/bin/env bats
# @test-type: integration — runs a real cargo build + codesign
# #3710 — retiered from "unit — hermetic source guard", which it never was: each
# case shells out to build-signed.sh, which compiles the crate and codesigns the
# result. That needs a Rust toolchain and, for the assertion to MEAN anything, a
# macOS keychain identity — cdhash stability under ad-hoc signing proves nothing
# about the TCC grant this exists to protect. #3684 already taught
# build-signed.sh to skip codesign off macOS, so on a Linux runner these cases
# burned minutes asserting a signature nobody applied. Integration tier keeps
# them in the local nightly, on the machine where the answer is real.
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
# #3997 — a test brings its own world (#3528): this suite asserts SIGNING, never
# deployment. Without this line, werk-test's FULL fallback ran these 8 builds
# with CHORUS_ROOT=$WERK and each INSTALLED the werk's binary into canonical
# ~/.chorus/bin and kickstarted the live daemon — six team-wide lockouts on
# 2026-08-23. The signing assertions below are unchanged by skipping install.
export BUILD_SKIP_INSTALL=1
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
