#!/usr/bin/env bats
# @test-type: unit — exercises the installer's refusal path; the REAL-home case never writes
# #3997 — test-context boundary on chorus-bin-install: a bats-run caller with the
# REAL user's $HOME may never install into canonical ~/.chorus/bin (the
# 2026-08-23 six-lockout class: a signing test deploying werk builds to prod).
# Negative proof at the BOUNDARY per Wren's review: the misconfigured test is
# shown REFUSED and canonical bin is byte-unchanged — not merely skipping.

INSTALL="$BATS_TEST_DIRNAME/../scripts/chorus-bin-install"

@test "BOUNDARY NEGATIVE PROOF: bats + real HOME + canonical target REFUSES (exit 11), real bin byte-unchanged, witnessed" {
  # candidate binary (content irrelevant — refusal must fire before any check moves bytes)
  printf '#!/bin/sh\necho werk-build\n' > "$BATS_TEST_TMPDIR/candidate"
  chmod +x "$BATS_TEST_TMPDIR/candidate"
  before="$(shasum -a 256 "$HOME/.chorus/bin/chorus-hooks" 2>/dev/null | awk '{print $1}')"
  SPINE="$BATS_TEST_TMPDIR/spine.log"
  # HOME deliberately left as the REAL home — this IS the misconfiguration
  CHORUS_BIN_SPINE_LOG="$SPINE" run bash "$INSTALL" "$BATS_TEST_TMPDIR/candidate" chorus-hooks
  [ "$status" -eq 11 ]
  [[ "$output" == *"REFUSED"* ]]
  after="$(shasum -a 256 "$HOME/.chorus/bin/chorus-hooks" 2>/dev/null | awk '{print $1}')"
  [ "$before" = "$after" ]
  grep -q 'binary.install.refused .*reason=test-context-canonical' "$SPINE"
}

@test "hermetic fixture (temp HOME) still installs under bats — the #3993 suite's world stays valid" {
  export HOME="$BATS_TEST_TMPDIR/home"
  mkdir -p "$HOME/.chorus/bin"
  cp /bin/ls "$BATS_TEST_TMPDIR/signed"; codesign -f -s - "$BATS_TEST_TMPDIR/signed" 2>/dev/null || skip "adhoc signing unavailable"
  CHORUS_BIN_SPINE_LOG="$BATS_TEST_TMPDIR/s.log" run bash "$INSTALL" "$BATS_TEST_TMPDIR/signed" chorus-hooks
  [ "$status" -eq 0 ]
}

@test "the signing suite itself no longer installs: BUILD_SKIP_INSTALL is pinned in-file" {
  grep -q 'export BUILD_SKIP_INSTALL=1' "$BATS_TEST_DIRNAME/chorus-inject-signed-stable.bats"
}
