#!/usr/bin/env bats
# @test-type: unit — hermetic: fake binaries in $BATS_TEST_TMPDIR, spine via CHORUS_BIN_SPINE_LOG
# #3993 — signed-install gate: chorus-bin-install must REFUSE an unsigned
# chorus-hooks/chorus-inject/chorus-hook-shim bound for canonical (launchd
# SIGKILLs them on spawn → team-wide lockout, 6x on 2026-08-23), keeping the
# previous binary. Negative proof per #3734: the violating state (unsigned
# candidate) is exercised and shown to FAIL the install.

INSTALL="$BATS_TEST_DIRNAME/../scripts/chorus-bin-install"

setup() {
  export HOME="$BATS_TEST_TMPDIR/home"
  mkdir -p "$HOME/.chorus/bin"
  export CHORUS_BIN_SPINE_LOG="$BATS_TEST_TMPDIR/spine.log"
  # an unsigned "binary": executable, but no Mach-O signature
  printf '#!/bin/sh\necho fake\n' > "$BATS_TEST_TMPDIR/fake-hooks"
  chmod +x "$BATS_TEST_TMPDIR/fake-hooks"
  # a pre-existing installed binary that must survive a refused install
  printf '#!/bin/sh\necho previous\n' > "$HOME/.chorus/bin/chorus-hooks"
  chmod +x "$HOME/.chorus/bin/chorus-hooks"
}

@test "NEGATIVE PROOF: unsigned chorus-hooks REFUSES (exit 10), previous binary kept, refusal witnessed" {
  command -v codesign >/dev/null || skip "codesign absent (non-macOS)"
  run bash "$INSTALL" "$BATS_TEST_TMPDIR/fake-hooks" chorus-hooks
  [ "$status" -eq 10 ]
  [[ "$output" == *"REFUSED"* ]]
  [ "$("$HOME/.chorus/bin/chorus-hooks")" = "previous" ]
  grep -q 'binary.install.refused .*binary=chorus-hooks reason=unsigned' "$CHORUS_BIN_SPINE_LOG"
}

@test "signed binary passes the gate (install proceeds)" {
  command -v codesign >/dev/null || skip "codesign absent (non-macOS)"
  # a real Mach-O, copied out of SIP territory and ad-hoc signed — codesign -v green
  cp /bin/ls "$BATS_TEST_TMPDIR/signed-hooks"
  codesign -f -s - "$BATS_TEST_TMPDIR/signed-hooks" 2>/dev/null || skip "adhoc signing unavailable"
  run bash "$INSTALL" "$BATS_TEST_TMPDIR/signed-hooks" chorus-hooks
  [ "$status" -eq 0 ]
  grep -q 'binary.deployed' "$CHORUS_BIN_SPINE_LOG"
}

@test "non-constrained binary (test-helper) is untouched by the gate" {
  run bash "$INSTALL" "$BATS_TEST_TMPDIR/fake-hooks" some-helper
  [ "$status" -eq 0 ]
}

@test "loud escape: CHORUS_BIN_SKIP_SIGCHECK=1 installs but says so on stderr" {
  CHORUS_BIN_SKIP_SIGCHECK=1 run bash "$INSTALL" "$BATS_TEST_TMPDIR/fake-hooks" chorus-hooks
  [ "$status" -eq 0 ]
  [[ "$output" == *"SIGCHECK SKIPPED"* ]]
}
