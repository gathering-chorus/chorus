#!/usr/bin/env bats
# @test-type: unit — reads the model files with arq and shacl; no store
# @domain: persistence
#
# #4422 — the persistence domain. A store row has exactly one parent (onDisk for a
# dataset, file or repo; inStore for a graph family) and exactly one role, with a
# role source unless it is the system of record. Rows are crawled, never deployed
# from a file (AC5). Model files read:
#   roles/silas/ontology/chorus.ttl
#   roles/wren/ontology/domains-wren-silas.ttl
# Fixtures:
#   platform/tests/fixtures/persistence-4422/good.ttl
#   platform/tests/fixtures/persistence-4422/bad-two-parents.ttl
#   platform/tests/fixtures/persistence-4422/bad-no-parent.ttl
#   platform/tests/fixtures/persistence-4422/bad-copy-no-source.ttl
#   platform/tests/fixtures/persistence-4422/bad-record-with-source.ttl

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
DOMAINS="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
FIX="$ROOT/platform/tests/fixtures/persistence-4422"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#>'

ask() { # ask <query body> <files…> → prints true or false
  local q="$1"; shift
  local data=(); for f in "$@"; do data+=(--data "$f"); done
  arq "${data[@]}" --results=csv "$P ASK { $q }" 2>/dev/null | tail -1 | tr -d '\r'
}

setup_file() {
  # the PersistenceStore shape alone, so the fixtures are judged by it and nothing else
  SHAPE="$BATS_FILE_TMPDIR/shape.ttl"
  { grep '^@prefix' "$MODEL"
    awk '/^chorus:PersistenceStoreShape a sh:NodeShape/{p=1} p{print} p && /\. *$/ && !/^ *#/{exit}' "$MODEL"; } > "$SHAPE"
  export SHAPE
}

conforms() { shacl validate --shapes "$SHAPE" --data "$1" 2>&1 | grep -oE 'sh:conforms +(true|false)' | awk '{print $2}'; }

@test "the shape was found in the model" {
  grep -q 'sh:targetClass chorus:PersistenceStore' "$SHAPE"
}

@test "a graph family in its dataset, copied from git, conforms" {
  [ "$(conforms "$FIX/good.ttl")" = "true" ]
}

@test "NEGATIVE PROOF: a graph row with both onDisk and inStore is refused" {
  [ "$(conforms "$FIX/bad-two-parents.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a row with neither onDisk nor inStore is refused" {
  [ "$(conforms "$FIX/bad-no-parent.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a copy that names no source is refused" {
  [ "$(conforms "$FIX/bad-copy-no-source.ttl")" = "false" ]
}

@test "NEGATIVE PROOF: a system of record that names a source is refused" {
  [ "$(conforms "$FIX/bad-record-with-source.ttl")" = "false" ]
}

@test "persistence is the one domain that claims PersistenceStore, and it sits in the chain" {
  [ "$(ask 'chorus:persistence chorus:definesVocabulary chorus:PersistenceStore ; chorus:atStep ?s ; chorus:ownedBy ?o .
            chorus:PersistenceStore chorus:inHierarchy "chain" ; chorus:parentVia chorus:onDisk , chorus:inStore .
            FILTER NOT EXISTS { ?d chorus:definesVocabulary chorus:PersistenceStore FILTER(?d != chorus:persistence) }' "$MODEL" "$DOMAINS")" = "true" ]
}

no_seeded_stores() { # AC5: no model or seed file carries a PersistenceStore row
  [ "$(ask '?s a chorus:PersistenceStore' "$@")" = "false" ]
}

model_files() { ls "$ROOT"/roles/*/ontology/*.ttl "$ROOT"/designing/data/*.ttl 2>/dev/null; }

@test "AC5: no file in the model or seed set writes a PersistenceStore row" {
  run model_files; [ "${#lines[@]}" -gt 10 ]
  no_seeded_stores $(model_files)
}

@test "NEGATIVE PROOF: a seed file with a store row fails the AC5 check" {
  bad="$BATS_TEST_TMPDIR/seed.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:persistence-store-x a chorus:PersistenceStore .\n' > "$bad"
  run no_seeded_stores $(model_files) "$bad"
  [ "$status" -ne 0 ]
}
