#!/usr/bin/env bats
# @test-type: unit — stand-in git and dagu record their calls; no live scheduler
# @domain: pipelines
# @card: 4579 · owner: kade
# What happened (10-18 20:06): #4579 changed the model (a .ttl and a
# model-retirements.jsonl line) and was started with werk-v2, the cicd
# pipeline. The athena pipeline's staging rehearsal (copy, deploy-seed,
# validate) never ran, so the retirement's damage went straight to prod.
# Now cicd refuses a card whose diff changes the model unless the athena
# pipeline is the one starting it (athena.yaml hands off to werk-v2 itself).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks"
  W="$CHORUS_WERK_BASE/kade-4579"
  mkdir -p "$CHORUS_HOME" "$W/platform/pipelines" "$CHORUS_WERK_BASE/kade-bin" "$BATS_TEST_TMPDIR/bin"
  touch "$W/platform/pipelines/cicd.yaml"
  export DAGU="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
  export PATH="$BATS_TEST_TMPDIR/bin:$PATH"
  unset DAG_NAME
}

diff_is() { # the stand-in git answers the card's diff with these paths
  printf '#!/bin/sh\nprintf "%%s\\n" %s\n' "$*" > "$BATS_TEST_TMPDIR/bin/git"; chmod +x "$BATS_TEST_TMPDIR/bin/git"
}

@test "negative proof: a .ttl change started as cicd is refused, nothing enqueued" {
  diff_is roles/kade/ontology/werk-domains.ttl platform/api/src/server.ts
  run "$ROOT/platform/scripts/werk-v2" 4579 kade
  [ "$status" -ne 0 ]
  [[ "$output" == *"changes the model (roles/kade/ontology/werk-domains.ttl)"*"chorus-athena"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "negative proof: a retirement line alone is a model change too" {
  diff_is designing/schemas/model-retirements.jsonl
  run "$ROOT/platform/scripts/werk-v2" 4579 kade
  [ "$status" -ne 0 ]
  [[ "$output" == *"designing/schemas/model-retirements.jsonl"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "the athena pipeline's own hand-off starts cicd for a model change" {
  diff_is roles/kade/ontology/werk-domains.ttl
  DAG_NAME=athena run "$ROOT/platform/scripts/werk-v2" 4579 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$(cat "$BATS_TEST_TMPDIR/args")" == *" enqueue $W/platform/pipelines/cicd.yaml -- CARD=4579 ROLE=kade" ]] || return 1
}

@test "a card with no model change starts as cicd" {
  diff_is platform/services/athena-deploy/src/lib.rs
  run "$ROOT/platform/scripts/werk-v2" 4579 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -f "$BATS_TEST_TMPDIR/args" ]
}
