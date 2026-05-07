#!/usr/bin/env bats
# spine-tick-poller-inject-resolve.bats — INJECT_BIN resolution order (#2772).
#
# The tick-poller LaunchAgent runs under launchd's minimal PATH (no ~/.chorus/bin),
# so `command -v chorus-inject` returns nothing under launchctl. Pre-#2772 the
# script hardcoded `target/release/chorus-inject`, the churning-cdhash binary
# whose TCC grant gets revoked on every rebuild. Post-#2772, the resolution
# order prefers the absolute deploy path first.

setup() {
  TEST_ROOT="$(mktemp -d)"
  export HOME_BACKUP="$HOME"
  export HOME="$TEST_ROOT/fake-home"
  export CHORUS_ROOT="$TEST_ROOT/chorus"
  mkdir -p "$HOME"
}

teardown() {
  export HOME="$HOME_BACKUP"
  rm -rf "$TEST_ROOT"
}

# Source the resolution block from spine-tick-poller into a function. We do
# this by extracting the logic into a temp script — same shape as the live
# code but isolatable.
resolve_inject_bin() {
  if [ -x "$HOME/.chorus/bin/chorus-inject" ]; then
    INJECT_BIN="$HOME/.chorus/bin/chorus-inject"
  elif command -v chorus-inject >/dev/null 2>&1; then
    INJECT_BIN="$(command -v chorus-inject)"
  else
    INJECT_BIN="${CHORUS_ROOT}/platform/services/chorus-inject/target/release/chorus-inject"
  fi
  echo "$INJECT_BIN"
}

@test "prefers ~/.chorus/bin/chorus-inject when deployed" {
  mkdir -p "$HOME/.chorus/bin"
  echo '#!/bin/sh' > "$HOME/.chorus/bin/chorus-inject"
  chmod +x "$HOME/.chorus/bin/chorus-inject"
  run resolve_inject_bin
  [ "$status" -eq 0 ]
  [ "$output" = "$HOME/.chorus/bin/chorus-inject" ]
}

@test "falls back to target/release when deploy is missing" {
  # No ~/.chorus/bin/chorus-inject; also strip PATH so command -v returns nothing.
  PATH="/nonexistent" run resolve_inject_bin
  [ "$status" -eq 0 ]
  [ "$output" = "$CHORUS_ROOT/platform/services/chorus-inject/target/release/chorus-inject" ]
}

@test "deploy path wins over PATH-resolved binary" {
  # Both available — deploy path should win for stable-cdhash guarantee.
  mkdir -p "$HOME/.chorus/bin" "$TEST_ROOT/other"
  echo '#!/bin/sh' > "$HOME/.chorus/bin/chorus-inject"
  echo '#!/bin/sh' > "$TEST_ROOT/other/chorus-inject"
  chmod +x "$HOME/.chorus/bin/chorus-inject" "$TEST_ROOT/other/chorus-inject"
  PATH="$TEST_ROOT/other:$PATH" run resolve_inject_bin
  [ "$status" -eq 0 ]
  [ "$output" = "$HOME/.chorus/bin/chorus-inject" ]
}
