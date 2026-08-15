#!/usr/bin/env bats
# @test-type: security — asserts the DAL gate on the instance-seed path.
#
# #3895 — the counterpart to recovery-path-ungated-3785.bats. That file proves
# the RECOVERY path (chorus-model-deploy.sh) never gates on identity. This file
# proves the split did not defang the OTHER side: the instance-seed leg that
# moved OUT of it (athena-seed.sh) still goes through the verified-identity DAL
# (#3687) and fails CLOSED without a token. Together they hold #3895's line:
# recovery ungated, instances gated, one file each.

setup() {
  ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  SEED="$ROOT/platform/scripts/athena-seed.sh"
}

@test "the instance-seed path exists where werk-deploy invokes it" {
  [ -f "$SEED" ]
}

@test "athena-seed writes through the governed DAL, not around it" {
  # Comment-stripped, same discipline as the 3785 guard: prose mentions in
  # comments must not satisfy a check about actual invocations.
  run bash -c "sed 's/#.*//' '$SEED' | grep -nE '(^|[^-a-z])athena-model[[:space:]]+seed'"
  [ "$status" -eq 0 ]
}

@test "athena-seed carries the identity gate the recovery path may not" {
  run bash -c "sed 's/#.*//' '$SEED' | grep -n 'CHORUS_IDENTITY_TOKEN'"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: unauthenticated athena-seed REFUSES — exit 1, nothing seeded" {
  # A world with no credential: HOME is an empty tmpdir (no ~/.chorus/identity/
  # cred for any role), no token in the environment, a role that cannot mint.
  # The gate must refuse BEFORE any write is attempted.
  unset CHORUS_IDENTITY_TOKEN
  run env -u CHORUS_IDENTITY_TOKEN HOME="$BATS_TEST_TMPDIR" \
    CHORUS_ROOT="$ROOT" DEPLOY_ROLE="no-such-role-3895" \
    bash "$SEED"
  [ "$status" -eq 1 ]
  [[ "$output" == *"cannot mint a CSS identity token"* ]]
}

@test "NEGATIVE PROOF: the invocation grep can fail — a file WITHOUT the call is not matched" {
  # Proves test 2 passing means presence, not blindness: a script that only
  # MENTIONS the writer in a comment must not satisfy it.
  FIXTURE="$BATS_TEST_TMPDIR/no-dal.sh"
  printf '#!/usr/bin/env bash\n# athena-model seed is mentioned only in prose\necho hello\n' > "$FIXTURE"
  run bash -c "sed 's/#.*//' '$FIXTURE' | grep -nE '(^|[^-a-z])athena-model[[:space:]]+seed'"
  [ "$status" -ne 0 ]
}
