#!/usr/bin/env bats
# @test-type: integration
# @domain: tests — the product domain this suite guards (#4334)
# #3918 — the land lane's own telemetry reaches the spine; a test's does not.
#
# This suite exists because the membrane could not separate the two states it
# is there to separate: act sets CI=true, the membrane called the werk pipeline
# a BUILD context, and every spine event the gate emitted panicked on the way
# out. The land happened; the record did not. Both directions are asserted here
# because fixing one by breaking the other is the failure mode (#3734).
load test_helper

CHORUS_LOG_BIN="${CHORUS_ROOT}/platform/scripts/chorus-log"

@test "land lane under act (CI set, CHORUS_CONTEXT=prod) reaches the spine" {
  run env CI=true CHORUS_CONTEXT=prod CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log" \
    BATS_TEST_TMPDIR= "$CHORUS_LOG_BIN" test.scoped silas card=3918 marker=lane-$$
  [ "$status" -eq 0 ]
  [[ "$output" != *"MEMBRANE REFUSED"* ]] || return 1
}

@test "NEGATIVE: a test context is STILL refused the production spine" {
  # No CHORUS_LOG_FILE override, so the surface resolves production — which is
  # exactly what must be refused. If this ever passes, #3615's teeth are gone.
  run env CI=true CHORUS_CONTEXT= BATS_TEST_TMPDIR="$BATS_TEST_TMPDIR" \
    CHORUS_LOG_FILE= "$CHORUS_LOG_BIN" test.scoped silas card=3918 marker=test-$$
  [[ "$output" == *"MEMBRANE REFUSED"* ]] || return 1
}

# #4336 — was a grep of werk.yml for a `CHORUS_CONTEXT= ... werk-test` line.
# Now the `test` step's own run script is pulled out of werk.yml and EXECUTED
# with the job's env, against stubs (resume-check, git, gh, werk-demo, and a
# werk-test that records the context it was handed). The property is what
# werk-test actually receives, not how the YAML happens to be spelled.
GHA_STEP="${CHORUS_ROOT}/platform/tests/fixtures/4336/gha-step.py"

# run_test_step <workflow.yml> — runs the `test` step; echoes what werk-test saw.
run_test_step() {
  local yml="$1" T="$BATS_TEST_TMPDIR/lane"
  # .nvm/…/v20 exists on every real runner; the step's node-pin line fails
  # under `bash -eo pipefail` without it (a harness artefact, not the property).
  mkdir -p "$T/home/.local/bin" "$T/home/.nvm/versions/node/v20.0.0/bin" \
    "$T/chome/platform/scripts" "$T/werks/kade-4336"
  local ctx
  ctx="$(python3 "$GHA_STEP" "$yml" env werk CHORUS_CONTEXT)" || return 1
  python3 "$GHA_STEP" "$yml" run test > "$T/step.sh" || return 1
  local b="$T/home/.local/bin"
  # resume-check exit 1 = "no carried pass" → the step proceeds to the real run.
  printf '#!/bin/bash\nexit 1\n' > "$T/chome/platform/scripts/werk-resume-check"
  printf '#!/bin/bash\necho deadbeef\n' > "$b/git"
  printf '#!/bin/bash\nexit 0\n' > "$b/gh"
  printf '#!/bin/bash\nexit 0\n' > "$b/werk-demo"
  printf '#!/bin/bash\necho "ctx=${CHORUS_CONTEXT-<unset>} root=${CHORUS_ROOT-<unset>}" >> "%s/werk-test.seen"\nexit 0\n' "$T" > "$b/werk-test"
  chmod +x "$b"/* "$T/chome/platform/scripts/werk-resume-check"
  rm -f "$T/werk-test.seen"
  env -i PATH="$b:/usr/bin:/bin" HOME="$T/home" CI=true CHORUS_CONTEXT="$ctx" \
    CARD_ID=4336 ROLE=kade CHORUS_HOME="$T/chome" CHORUS_WERK_BASE="$T/werks" \
    bash -eo pipefail "$T/step.sh" >"$T/step.out" 2>&1 || { cat "$T/step.out"; return 1; }
  cat "$T/werk-test.seen"
}

@test "the werk.yml test step hands werk-test the runner's prod context (step executed)" {
  run run_test_step "${CHORUS_ROOT}/.github/workflows/werk.yml"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "ctx=prod root=$BATS_TEST_TMPDIR/lane/werks/kade-4336" ]
}

@test "NEGATIVE: a test step that clears CHORUS_CONTEXT for werk-test is seen as cleared" {
  # The #3918 regression, re-introduced in a copy: the harness must tell it apart.
  bad="$BATS_TEST_TMPDIR/werk-cleared.yml"
  sed 's|^\([[:space:]]*\)CHORUS_ROOT="\$WERK" PATH=\(.*\)werk-test |\1CHORUS_CONTEXT= CHORUS_ROOT="$WERK" PATH=\2werk-test |' \
    "${CHORUS_ROOT}/.github/workflows/werk.yml" > "$bad"
  ! cmp -s "$bad" "${CHORUS_ROOT}/.github/workflows/werk.yml" || return 1
  run run_test_step "$bad"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == "ctx= root="* ]] || return 1
}
