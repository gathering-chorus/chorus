#!/usr/bin/env bats
# @test-type: unit — reads the model files with arq; no store
# @domain: infrastructure
#
# #4485 — machines and disks sit in the chain under the one-parent rule (#4481):
# a Machine's parent is the infrastructure domain that hosts it, a Disk's is its
# Machine. The Machine rows carry names only; everything else is measured by
# the crawl. Model files read:
#   roles/silas/ontology/chorus.ttl
#   roles/wren/ontology/domains-wren-silas.ttl
#   designing/data/machine-instances.ttl

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
DOMAINS="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
MACHINES="$ROOT/designing/data/machine-instances.ttl"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>'

ask() { # ask <query body> <files…> → prints true or false
  local q="$1"; shift
  local data=(); for f in "$@"; do data+=(--data "$f"); done
  arq "${data[@]}" --results=csv "$P ASK { $q }" 2>/dev/null | tail -1 | tr -d '\r'
}

@test "Disk is in the chain and its parent is its Machine" {
  [ "$(ask 'chorus:Disk chorus:inHierarchy "chain" ; chorus:parentVia chorus:onMachine . chorus:DiskShape sh:targetClass chorus:Disk' "$MODEL")" = "true" ]
}

@test "a Machine's parent is the infrastructure domain that hosts it" {
  [ "$(ask 'chorus:Machine chorus:inHierarchy "chain" ; chorus:parentVia [ sh:inversePath chorus:hosts ] .
            chorus:infrastructure chorus:hosts chorus:library , chorus:bedroom ; chorus:definesVocabulary chorus:Disk' "$MODEL" "$DOMAINS")" = "true" ]
}

machines_carry_no_prose() {
  [ "$(ask '?m a chorus:Machine ; rdfs:comment ?c' "$1")" = "false" ]
}

@test "Machine rows carry names only; the crawl measures the rest" {
  machines_carry_no_prose "$MACHINES"
}

@test "NEGATIVE PROOF: a hand-typed note on a Machine row fails the check" {
  bad="$BATS_TEST_TMPDIR/machines.ttl"
  sed 's|rdfs:label "Library" \.|rdfs:label "Library" ; rdfs:comment "M1 MacBook Air." .|' "$MACHINES" > "$bad"
  grep -q 'MacBook' "$bad"
  run machines_carry_no_prose "$bad"
  [ "$status" -ne 0 ]
}

owner_rule() { # the athena-validate owner-rule-missing question, asked of one shape file
  [ "$(ask "chorus:$2Shape sh:property [ sh:path chorus:ownedBy ; sh:class chorus:Principal ]" "$1")" = "true" ]
}

@test "Machine and Disk say what an owner is (owner-rule-missing)" {
  owner_rule "$MODEL" Machine
  owner_rule "$MODEL" Disk
}

@test "NEGATIVE PROOF: a Disk shape without the owner rule fails the check" {
  bad="$BATS_TEST_TMPDIR/chorus.ttl"
  # the Disk shape's owner path renamed: still valid turtle, no longer the owner rule
  awk '/^chorus:DiskShape a sh:NodeShape/{d=1} d && /sh:path chorus:ownedBy /{sub(/chorus:ownedBy /,"chorus:notOwnedBy ");d=0} {print}' "$MODEL" > "$bad"
  grep -q 'chorus:notOwnedBy' "$bad"
  riot --validate "$bad" >/dev/null 2>&1
  run owner_rule "$bad" Disk
  [ "$status" -ne 0 ]
}
