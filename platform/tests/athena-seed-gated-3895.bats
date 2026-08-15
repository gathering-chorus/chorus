#!/usr/bin/env bats
# @test-type: security — asserts the DAL gate on the instance-seed path.
#
# #3895 — the counterpart to recovery-path-ungated-3785.bats. That file proves
# the RECOVERY path (chorus-model-deploy.sh) never gates on identity. This file
# proves the split did not defang the OTHER side: the instance-seed leg that
# moved OUT of it — `athena-model seed --deploy` (Rust, ADR-038: no deploy-path
# bash) — still goes through the verified-identity DAL (#3687) and fails CLOSED
# without a token. Together they hold #3895's line: recovery ungated, instances
# gated, one home each.

setup() {
  ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  MANIFEST="$ROOT/platform/config/instance-seed-manifest.txt"
  # The artifact under test: this tree's build when present (a werk proves its
  # OWN binary, not whatever older build is installed), installed otherwise.
  BIN="$ROOT/platform/services/chorus-model/target/release/athena-model"
  [ -x "$BIN" ] || BIN="$HOME/.chorus/bin/athena-model"
}

@test "the seed manifest exists and declares at least one kind:path group" {
  [ -f "$MANIFEST" ]
  run bash -c "grep -cE '^[a-z-]+:.+\.ttl$' '$MANIFEST'"
  [ "$status" -eq 0 ]
  [ "$output" -ge 1 ]
}

@test "the seeder binary exists where the land path invokes it" {
  [ -x "$BIN" ]
}

@test "NEGATIVE PROOF: unauthenticated seed --deploy REFUSES — exit nonzero, names the gate" {
  # A world with no credential: token absent from the environment. The DAL
  # refuses BEFORE any write (#3687 fail-closed) — this is the runtime proof
  # the split kept the gate.
  run env -u CHORUS_IDENTITY_TOKEN CHORUS_ROOT="$ROOT" "$BIN" seed --deploy
  [ "$status" -ne 0 ]
  [[ "$output" == *"identity-token-required"* ]]
}

@test "NEGATIVE PROOF: a BOGUS token is refused too — presence is not verification" {
  # A token that is present but does not verify must fail closed, not degrade.
  run env CHORUS_IDENTITY_TOKEN="not-a-real-token" CHORUS_ROOT="$ROOT" "$BIN" seed --deploy
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: an empty manifest is refused, never a vacuous green" {
  # A deploy that seeds nothing because its manifest went missing must say so.
  FAKEROOT="$BATS_TEST_TMPDIR/root"
  mkdir -p "$FAKEROOT/platform/config"
  printf '# no groups here\n' > "$FAKEROOT/platform/config/instance-seed-manifest.txt"
  run env -u CHORUS_IDENTITY_TOKEN CHORUS_ROOT="$FAKEROOT" "$BIN" seed --deploy
  [ "$status" -ne 0 ]
  [[ "$output" == *"no groups"* ]]
}
