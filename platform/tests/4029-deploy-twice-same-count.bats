#!/usr/bin/env bats
# @test-type: integration — deploys a fixture TTL into a THROWAWAY graph on the live Fuseki (skip-if-absent)
# #4029 — the model deploy re-inserted every shape body on every run: a shape body is a
# blank-node tree, blank nodes get a fresh identity per load, the merge only deleted a
# staged subject's own triples, so 92 deploys took urn:chorus:ontology 5,230 → 77,770
# with no new content. Kade's proof (2026-08-28): deploy twice, second count equals the
# first. Negative proof (#3734): with the blank-node cleanup switched off, the second
# count is LARGER — the check can go red.
# Isolation: every write targets urn:chorus:ontology-test-bats-4029 (wipe-guard scan).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/athena-deploy-model.sh"
GRAPH="urn:chorus:ontology-test-bats-4029-$$"   # #4084: per-process suffix — two pipelines running this at once shared one graph and tore each other down
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"

setup() {
  curl -s -o /dev/null --max-time 3 "http://localhost:3030/\$/ping" || skip "fuseki not running"
  source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  TTL="$BATS_TEST_TMPDIR/shapes.ttl"
  cat > "$TTL" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh:     <http://www.w3.org/ns/shacl#> .
@prefix owl:    <http://www.w3.org/2002/07/owl#> .
@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .
chorus:Bats4029Domain a owl:Class, chorus:Domain ; rdfs:label "bats-4029" .
chorus:Bats4029Shape a sh:NodeShape ;
  sh:targetClass chorus:Bats4029Domain ;
  sh:property [ sh:path rdfs:label ; sh:minCount 1 ; sh:or ( [ sh:datatype <http://www.w3.org/2001/XMLSchema#string> ] [ sh:nodeKind sh:Literal ] ) ] ;
  sh:property [ sh:path chorus:purpose ; sh:maxCount 1 ] .
TTL
}

teardown() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$GRAPH" -o /dev/null 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=${GRAPH}-staging-deploy" -o /dev/null 2>/dev/null || true
}

count() {
  curl -s "$Q" --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$GRAPH> { ?s ?p ?o } }" -H 'Accept: text/csv' | tail -1 | tr -dc '0-9'
}

# The assertions are on COUNTS, never on the script's exit code: with the cleanup
# switched off the script may legitimately refuse its own post-merge verify (that
# is the defect), and a refusal must not abort the test before the count is read.
deploy() { env ONTOLOGY_GRAPH="$GRAPH" TTL="$TTL" "$@" bash "$SCRIPT" >> "$BATS_TEST_TMPDIR/deploy.log" 2>&1 || true; }

@test "deploy twice from the same source: the second count equals the first" {
  deploy; a=$(count)
  deploy; b=$(count)
  [ -n "$a" ] && [ "$a" -gt 0 ]
  [ "$a" = "$b" ]
}

@test "NEGATIVE PROOF: with the blank-node cleanup off, the second deploy GROWS the graph" {
  deploy DEPLOY_BNODE_CLEANUP=0; a=$(count)
  deploy DEPLOY_BNODE_CLEANUP=0; b=$(count)
  [ "$b" -gt "$a" ]
}

@test "the fixed merge also repairs a graph that already carries duplicated bodies" {
  deploy DEPLOY_BNODE_CLEANUP=0; deploy DEPLOY_BNODE_CLEANUP=0; inflated=$(count)
  deploy; fixed=$(count)
  [ "$fixed" -lt "$inflated" ]
  deploy; again=$(count)
  [ "$fixed" = "$again" ]
}
