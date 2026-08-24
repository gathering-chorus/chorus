#!/usr/bin/env bats
# @test-type: unit:security — static assertions on the recovery script itself;
# no service, store or credential is exercised (signal:security is the subject).
#
# #3785 — the recovery path must never gate on identity.
#
# On 2026-08-06 the allow-set was emptied. Jeff lost the Clearing, and the
# governed writer refused the operator who had just deleted his own WebID —
# athena-model returned WebIdNotAllowed to the one person who could put the
# records back. Recovery worked ONLY because chorus-model-deploy authenticates
# to the store rather than through the door.
#
# That was luck wearing the costume of design (Wren, same day). This makes it
# design: if the recovery path ever grows an identity check, the system becomes
# unrecoverable from its own most likely security failure, and it would happen
# invisibly — the gate would look like every other gate, correct in isolation.
#
# The check is deliberately crude. A sophisticated one would drift; this one
# fails loudly the moment someone adds a token requirement to the recovery path.

setup() {
  ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"   # #3904: derive, never hardcode a /Users path
  DEPLOY="$ROOT/platform/scripts/athena-deploy-model.sh"
}

@test "the recovery path exists where the incident runbook says it does" {
  [ -f "$DEPLOY" ]
}

@test "recovery authenticates to the STORE — it reads Fuseki credentials" {
  grep -q "FUSEKI_ADMIN_USER\|fuseki-auth" "$DEPLOY"
}

@test "NEGATIVE PROOF: recovery does NOT require an identity token" {
  # CHORUS_IDENTITY_TOKEN is what the DAL demands (#3687). If the recovery path
  # ever requires it, emptying the allow-set becomes unrecoverable by anyone who
  # was in it — which is everyone who could fix it.
  run grep -n "CHORUS_IDENTITY_TOKEN" "$DEPLOY"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: recovery does not CALL the identity-gated writer" {
  # athena-model refuses on WebIdNotAllowed. A recovery path that shells to it
  # inherits the lockout it exists to undo.
  #
  # Comments stripped first: the first version of this test grepped the raw file
  # and failed on two PROSE mentions of athena-model in comments. A check that
  # cannot tell a comment from a command cannot tell the two states it exists to
  # separate — the same defect this file exists to prevent, reproduced inside it.
  run bash -c "sed 's/#.*//' '$DEPLOY' | grep -nE '(^|[^-a-z])athena-model[[:space:]]+(add|set|seed|delete|link|unlink|mint)'"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: the comment-stripped check still catches a REAL call" {
  # Proves the strip did not defang the check — plant an actual invocation and
  # confirm it is seen, so the test above passing means absence rather than
  # blindness.
  FIXTURE="$BATS_TEST_TMPDIR/calls-writer.sh"
  printf '#!/usr/bin/env bash\n# athena-model add is mentioned here in prose\nathena-model add --kind principal --name x\n' > "$FIXTURE"
  run bash -c "sed 's/#.*//' '$FIXTURE' | grep -nE '(^|[^-a-z])athena-model[[:space:]]+(add|set|seed|delete|link|unlink|mint)'"
  [ "$status" -eq 0 ]
}

@test "the allow-set gate itself never gates on identity" {
  # The gate runs during deploy, including the deploy that would RESTORE an
  # emptied allow-set. If it demanded a token it would refuse exactly when it
  # was most needed.
  GATE="$ROOT/platform/scripts/chorus-allow-set-gate"
  [ -f "$GATE" ]
  run grep -n "CHORUS_IDENTITY_TOKEN" "$GATE"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: this check can fail — a fixture WITH a token gate is caught" {
  # Without this, every assertion above would pass against a file that simply
  # did not exist, or against a check too weak to see a violation. Plant the
  # violation and prove the grep catches it.
  FIXTURE="$BATS_TEST_TMPDIR/fake-recovery.sh"
  printf '#!/usr/bin/env bash\n[ -n "$CHORUS_IDENTITY_TOKEN" ] || exit 1\n' > "$FIXTURE"
  run grep -n "CHORUS_IDENTITY_TOKEN" "$FIXTURE"
  [ "$status" -eq 0 ]
}
