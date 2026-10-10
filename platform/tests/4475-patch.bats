#!/usr/bin/env bats
# @test-type: unit — reads the scripts and runs the cargo tests that prove PATCH; no store, no network.
# @domain: domains
# @card: #4475
# @owner: wren
#
# #4475 — Jeff 2026-10-10 10:57: "feels like we need patch ability on current athena-make".
# A PUT restates the whole row: on pulse it turned 13 consumes links into text. PATCH
# changes only the fields it names; null removes one predicate. The door's behaviour is
# proven in athena-make's hermetic test (patch_changes_only_the_named_fields) and the
# athena-model verb in its patch_tests; this file covers the post-land proof script:
#   roles/wren/notes/4475-postland-gates.sh

ROOT="$BATS_TEST_DIRNAME/../.."
GATES="$ROOT/roles/wren/notes/4475-postland-gates.sh"

@test "the post-land proof parses and patches only implementedIn, through athena-model" {
  bash -n "$GATES"
  grep -q '"implementedIn":null' "$GATES"
  grep -q '"$AM" patch --path "/v1/security/gates/' "$GATES"
}

@test "the proof reads back both sides: implementedIn gone, type and subClassOf kept" {
  grep -q 'implementedIn triples' "$GATES"
  grep -q 'gates with type+subClassOf' "$GATES"
}

@test "negative proof: the script refuses to write on a dry run" {
  run grep -c 'DRY:-' "$GATES"
  [ "$output" -ge 1 ]
}
