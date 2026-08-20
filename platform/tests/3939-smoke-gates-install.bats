#!/usr/bin/env bats
# @test-type: unit — hermetic: fake HOME, fixture binaries, spine to a temp log
#
# #3939 — chorus-bin-install refuses a candidate werk verb that fails its smoke.
# The guarded condition: a broken binary becoming the running binary. Before
# this, a Jul-24 werk-commit ran the live pipeline for a month past its fix.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  export HOME="$T/home"; mkdir -p "$HOME"
  export CHORUS_BIN_SPINE_LOG="$T/spine.log"
  export CHORUS_BIN_NO_KICKSTART=1
  INSTALL="$REPO/platform/scripts/chorus-bin-install"
}

teardown() { rm -rf "$T"; }

mkbin() { printf '#!/bin/sh\nexit %s\n' "$1" > "$T/$2"; chmod +x "$T/$2"; }

@test "a candidate that fails its smoke is REFUSED and the old binary survives" {
  # previous good binary in place
  mkdir -p "$HOME/.chorus/bin"; echo "OLD" > "$HOME/.chorus/bin/werk-frobnicate"
  mkbin 9 candidate            # bare-invoke crashes (rc>2) → smoke fails
  run bash "$INSTALL" "$T/candidate" werk-frobnicate
  [ "$status" -eq 8 ]
  [[ "$output" == *REFUSED* ]]
  [ "$(cat "$HOME/.chorus/bin/werk-frobnicate")" = "OLD" ]   # untouched
  grep -q 'binary.install.refused.*werk-frobnicate.*smoke-fail' "$CHORUS_BIN_SPINE_LOG"
}

@test "a candidate that passes its smoke installs with the deployed event" {
  mkbin 0 candidate            # bare-invoke exits 0 → typed-refusal contract ok
  run bash "$INSTALL" "$T/candidate" werk-frobnicate
  [ "$status" -eq 0 ]
  [ -x "$HOME/.chorus/bin/werk-frobnicate" ]
  grep -q 'binary.deployed.*werk-frobnicate' "$CHORUS_BIN_SPINE_LOG"
}

@test "non-werk binaries install without smoke (scope stays narrow)" {
  mkbin 9 candidate            # would fail a smoke — but not a werk verb
  run bash "$INSTALL" "$T/candidate" chorus-somethingelse
  [ "$status" -eq 0 ]
}

@test "missing verb-smoke refuses blind install rather than passing vacuously" {
  mkbin 0 candidate
  # point the install at a copy whose sibling verb-smoke does not exist
  mkdir -p "$T/scripts"; cp "$INSTALL" "$T/scripts/chorus-bin-install"
  run bash "$T/scripts/chorus-bin-install" "$T/candidate" werk-frobnicate
  [ "$status" -eq 9 ]
  grep -q 'binary.install.refused.*smoke-missing' "$CHORUS_BIN_SPINE_LOG"
}

@test "CHORUS_BIN_SKIP_SMOKE=1 bypasses loudly (bootstrap escape)" {
  mkbin 9 candidate
  CHORUS_BIN_SKIP_SMOKE=1 run bash "$INSTALL" "$T/candidate" werk-frobnicate
  [ "$status" -eq 0 ]
  [[ "$output" == *"SMOKE SKIPPED"* ]]
}
