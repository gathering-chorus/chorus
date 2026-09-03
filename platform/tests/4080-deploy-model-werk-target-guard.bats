#!/usr/bin/env bats
# @test-type: unit
# #4080 — athena-deploy-model.sh refuses a bare run from inside a werk, where its
# defaults point at PROD (localhost:3030/pods). Regression for 2026-09-03 07:29:
# a hand run from werk-silas rewrote prod's ontology graph (the 08-28
# werk-writes-prod class). Hermetic: ATHENA_DEPLOY_TARGET_CHECK_ONLY=1 exits right
# after the guard, so nothing is deployed anywhere. Negative proof (#3734): the
# guard is shown to REFUSE (exit 78, naming pods) from a werk path with no store
# named, and to PASS from the same path once the werk store is named, or when
# canonical is said on purpose, or from a non-werk root.
# wipe-guard: exempt — ATHENA_DEPLOY_TARGET_CHECK_ONLY=1 exits the script before
# any Fuseki call (the seam is above the FUSEKI_* defaults); ONTOLOGY_GRAPH is
# still pointed at a throwaway graph below so even a broken seam cannot reach live.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/athena-deploy-model.sh"

setup() {
  WERK_DIR="$BATS_TEST_TMPDIR/chorus-werk/wren-4080"
  mkdir -p "$WERK_DIR"
  export ATHENA_DEPLOY_TARGET_CHECK_ONLY=1
  export ONTOLOGY_GRAPH="urn:chorus:ontology-test-bats-4080"
}

@test "negative proof: bare run inside a werk is REFUSED and names prod" {
  run env -u FUSEKI_GSP -u DEPLOY_TARGET -u ONTOLOGY_GRAPH CHORUS_ROOT="$WERK_DIR" bash -c "cd '$WERK_DIR' && bash '$SCRIPT'"
  [ "$status" -eq 78 ]
  [[ "$output" == *"REFUSED"* ]]
  [[ "$output" == *"pods"* ]]
}

@test "inside a werk with the werk store named, the guard passes" {
  run env FUSEKI_GSP="http://localhost:3030/werk-wren/data" CHORUS_ROOT="$WERK_DIR" bash -c "cd '$WERK_DIR' && bash '$SCRIPT'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"target-check: ok"* ]]
  [[ "$output" == *"werk-wren"* ]]
}

@test "inside a werk with DEPLOY_TARGET=canonical said on purpose, the guard passes" {
  run env -u FUSEKI_GSP DEPLOY_TARGET=canonical CHORUS_ROOT="$WERK_DIR" bash -c "cd '$WERK_DIR' && bash '$SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "from a non-werk root the defaults stand (the canonical land path is untouched)" {
  NONWERK="$BATS_TEST_TMPDIR/chorus"; mkdir -p "$NONWERK"
  run env -u FUSEKI_GSP -u DEPLOY_TARGET CHORUS_ROOT="$NONWERK" bash -c "cd '$NONWERK' && bash '$SCRIPT'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"default pods"* ]]
}

@test "inside a werk, a test naming its own throwaway ONTOLOGY_GRAPH passes (the #3601 discipline)" {
  run env -u FUSEKI_GSP -u DEPLOY_TARGET ONTOLOGY_GRAPH="urn:chorus:ontology-test-bats-x" CHORUS_ROOT="$WERK_DIR" bash -c "cd '$WERK_DIR' && bash '$SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "negative proof: inside a werk, ONTOLOGY_GRAPH set to the LIVE graph is still REFUSED" {
  run env -u FUSEKI_GSP -u DEPLOY_TARGET ONTOLOGY_GRAPH="urn:chorus:ontology" CHORUS_ROOT="$WERK_DIR" bash -c "cd '$WERK_DIR' && bash '$SCRIPT'"
  [ "$status" -eq 78 ]
}
