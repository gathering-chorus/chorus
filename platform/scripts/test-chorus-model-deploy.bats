#!/usr/bin/env bats
# @test-type: integration
# test-chorus-model-deploy.bats (#3509) — the MODEL (chorus.ttl schema) deploys into Fuseki.
# AC: a deploy step loads chorus.ttl -> urn:chorus:ontology and the 4 primitive shapes
# (+ StepShape) are queryable live; an invalid model is refused fail-loud (no deploy).
# Runs against a throwaway test graph so it never touches the live ontology graph.

ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
SCRIPT="$ROOT/platform/scripts/chorus-model-deploy.sh"
TTL="$ROOT/roles/silas/ontology/chorus.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3509"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"

teardown() {
  curl -s -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-bad" -o /dev/null 2>/dev/null || true
}

@test "chorus-model-deploy loads chorus.ttl into the ontology graph (exit 0)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "all 4 primitive shapes + StepShape are queryable after deploy" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$TEST_GRAPH> { ?s a sh:NodeShape . FILTER(?s IN (chorus:ProductShape,chorus:DomainShape,chorus:ServiceShape,chorus:ValueStreamShape,chorus:StepShape)) } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"value":"5"'* ]]
}

@test "DomainShape carries chorus:purpose (the capability) after deploy" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> ASK { GRAPH <$TEST_GRAPH> { chorus:DomainShape sh:property [ sh:path chorus:purpose ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "an invalid TTL is refused fail-loud (exit 1, no deploy)" {
  badttl="$(mktemp)"; printf 'this is not @@ valid turtle .\n' > "$badttl"
  run env ONTOLOGY_GRAPH="${TEST_GRAPH}-bad" TTL="$badttl" bash "$SCRIPT"
  rm -f "$badttl"
  [ "$status" -eq 1 ]
}
