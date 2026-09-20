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
SCRIPT="$ROOT/platform/services/athena-deploy/target/release/athena-deploy"
load test_helper   # test_graph_name (run-scoped throwaway graph)
# #4084 gave this a per-PROCESS suffix; bats forks per @test, so teardown dropped
# a name that was never created and the real graphs leaked into the live store.
# Run-scoped instead: unique per run, identical across its processes.
GRAPH="$(test_graph_name 4029)"
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
  curl -s --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$GRAPH" -o /dev/null 2>/dev/null || true
  curl -s --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=${GRAPH}-staging-deploy" -o /dev/null 2>/dev/null || true
}

# A count that could not be MEASURED must say so, not come back empty. On #4175
# run 1 the store did not answer one of these calls under load; count() returned
# "" and the comparison died with "integer expression expected" — reported as a
# product break on a suite that passes by hand, 3/3, seconds later. The store
# being busy is not this deploy script being wrong, and a test that cannot tell
# those apart is the whack-a-mole shape. UNMEASURED fails the run loudly with
# the reason, and never reads as the defect the suite exists to catch.
count() {
  local csv n
  csv="$(curl -s --max-time 30 "$Q" --data-urlencode \
    "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$GRAPH> { ?s ?p ?o } }" \
    -H 'Accept: text/csv')" || csv=""
  # A well-formed answer is the header line "n" plus a numeric row. Anything
  # else — empty body, curl failure, an HTML error page — is unmeasured.
  n="$(printf '%s' "$csv" | tail -1 | tr -dc '0-9')"
  if [ -z "$n" ] || ! printf '%s' "$csv" | head -1 | grep -q '^n'; then
    echo "UNMEASURED: the store did not answer the triple count for <$GRAPH>." >&2
    echo "  raw response: $(printf '%s' "$csv" | head -3 | tr '\n' ' ')" >&2
    return 1
  fi
  printf '%s' "$n"
}

# The assertions are on COUNTS, never on the script's exit code: with the cleanup
# switched off the script may legitimately refuse its own post-merge verify (that
# is the defect), and a refusal must not abort the test before the count is read.
deploy() { env ONTOLOGY_GRAPH="$GRAPH" TTL="$TTL" "$@" "$SCRIPT" >> "$BATS_TEST_TMPDIR/deploy.log" 2>&1 || true; }

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
