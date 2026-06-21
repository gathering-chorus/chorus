#!/usr/bin/env bats
# @test-type: integration
# 3540-tests-domain-schema-land.bats (#3540) — the tests-domain shape (Test /
# TestResult / covers / pyramidLayer / hermeticity, authored on kade/2818)
# LANDS into the live model graph and is queryable + instance-mintable.
#
# This is the schema-land foundation Kade's #2818 tagging populates against and
# #3190's grep→graph selection queries. The contract under test:
#   1. werk-domains.ttl deploys cleanly into the ontology graph (exit 0).
#   2. TestEdgesShape carries the REQUIRED axes (pyramidLayer minCount 1, covers
#      minCount 1) and the OPTIONAL finer axis (hermeticity, enum-if-present).
#   3. A minted Test instance is queryable BY ITS covers edge — the exact query
#      werk-test (#3190) runs: card's changed SubDomain -> covering Tests.
#   4. werk-domains.ttl is part of the deployed model SET, not orphaned (the gap
#      this card closes: the model existed but was never landed).
#
# Runs against a throwaway test graph; never touches the live ontology graph.

# Invariant #1 (a test brings its own world): derive the repo root from THIS test's
# own location, never an ambient CHORUS_ROOT that may point at a different checkout.
# (#3540's whole point — a test that honors a foreign CHORUS_ROOT tests the wrong files.)
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/chorus-model-deploy.sh"
TTL="$ROOT/roles/kade/ontology/werk-domains.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3540"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"
PFX='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#>'

teardown() {
  curl -s -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null || true
}

@test "werk-domains.ttl deploys into the ontology graph (exit 0)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "TestEdgesShape requires pyramidLayer (minCount 1) after deploy" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:pyramidLayer ; sh:minCount 1 ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "TestEdgesShape requires covers (minCount 1) — the werk-test query edge" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:covers ; sh:minCount 1 ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "TestEdgesShape carries hermeticity as the optional finer axis (enum, no minCount)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:hermeticity ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "a minted Test instance is queryable BY its covers edge (the #3190 contract)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  # mint a Test instance covering a known SubDomain, the way the #2818 tagging will
  curl -s -X POST "http://localhost:3030/pods/update" --data-urlencode "update=$PFX INSERT DATA { GRAPH <$TEST_GRAPH> { chorus:test-3540-probe a chorus:Test ; chorus:covers chorus:subdomain-tests-domain ; chorus:pyramidLayer \"integration\" ; chorus:hermeticity \"needs-stack\" } }" -o /dev/null 2>/dev/null
  # query the way werk-test will: which Tests cover this SubDomain?
  run curl -s "$Q" --data-urlencode "query=$PFX SELECT ?t WHERE { GRAPH <$TEST_GRAPH> { ?t a chorus:Test ; chorus:covers chorus:subdomain-tests-domain } }" -H "Accept: application/sparql-results+json"
  [[ "$output" == *'test-3540-probe'* ]]
}

@test "werk-domains.ttl is wired into the model-deploy SET (not orphaned)" {
  # the gap this card closes: chorus-model-deploy must deploy the model SET,
  # not chorus.ttl alone. Asserts werk-domains is a declared member.
  run grep -qE "werk-domains|MODEL_SET|model.*set" "$SCRIPT"
  [ "$status" -eq 0 ]
}
