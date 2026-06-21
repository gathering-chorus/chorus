#!/usr/bin/env bats
# @test-type: integration
# 3550 — athena-deploy: a per-domain model deploy must NOT clobber a sibling
# domain's live data. Regression for the #3529 clobber: #3540's full-replace
# (COPY staging->ontology) wiped value-stream wiring that wasn't in the deployed
# TTL. The fix: delete-staged-subjects-then-insert (additive), so a deploy
# touches only the deploying domain's own subjects.
#
# Invariant #1 (a test brings its own world): root derives from this test's
# location; throwaway graph; never the live ontology.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/chorus-model-deploy.sh"
TTL="$ROOT/roles/kade/ontology/werk-domains.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3550"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"
UPD="http://localhost:3030/pods/update"
PFX='PREFIX chorus: <https://jeffbridwell.com/chorus#>'
SHPFX='PREFIX sh: <http://www.w3.org/ns/shacl#>'

teardown() { curl -s -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null || true; }

plant_sibling() {
  # mimic #3529: value-stream wiring loaded LIVE into the graph, NOT in any deployed TTL
  curl -s -X POST -H 'Content-Type: application/sparql-update' --data-binary \
    "$PFX INSERT DATA { GRAPH <$TEST_GRAPH> { chorus:vs-step-sibling-3550 a chorus:ValueStreamStep ; chorus:stepOrder 7 ; chorus:inStream chorus:vs-werk } }" "$UPD" -o /dev/null 2>/dev/null
}

@test "deploying a domain does NOT clobber a sibling's live triples (#3529 regression)" {
  plant_sibling
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:vs-step-sibling-3550 chorus:stepOrder 7 ; chorus:inStream chorus:vs-werk } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "deploying a domain still lands its OWN shape (additive merge works)" {
  plant_sibling
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX $SHPFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:hermeticity ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "re-deploying the SAME domain is idempotent (subject not duplicated)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  # pyramidLayer is declared `a owl:DatatypeProperty` exactly once after two deploys
  run curl -s "$Q" --data-urlencode "query=$PFX SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$TEST_GRAPH> { chorus:pyramidLayer a ?t } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"value":"1"'* ]]
}
