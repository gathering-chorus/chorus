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


# #3606 — source Fuseki auth. Fuseki refuses unauthenticated writes (401) and
# the setup writes below discard stderr, so a refused INSERT looked exactly like
# a successful one — the later assertion then failed on absent data and read as a
# logic bug. Same swallowed-401 as 3540.
#
# ROOT is derived before the source: with CHORUS_ROOT unset the old form sourced
# "/platform/scripts/fuseki-auth.sh", left FUSEKI_AUTH empty, and restored the
# exact swallowed-401 this comment describes.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# shellcheck source=/dev/null
. "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
# #3991: repointed — #3561 renamed chorus-model-deploy.sh → athena-deploy-model.sh
# and this suite kept exit-127ing on the dead path (guard-target-deleted class).
SCRIPT="$ROOT/platform/scripts/athena-deploy-model.sh"
TTL="$ROOT/roles/kade/ontology/werk-domains.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3550"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"
UPD="http://localhost:3030/pods/update"
PFX='PREFIX chorus: <https://jeffbridwell.com/chorus#>'
SHPFX='PREFIX sh: <http://www.w3.org/ns/shacl#>'

# #3606 — a drop that cannot be verified is not a drop. Adding auth (above) fixed
# the refusal but left `|| true`, so a failed clear still read as clean; 1,287
# triples were resident live. This suite's whole subject is what SURVIVES a deploy
# — it plants a sibling subject and asserts it is still there afterwards — so
# residue from a prior run is the one thing that can make the clobber-regression
# pass while the clobber is happening.
_drop_test_graph() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$Q" -H "Accept: text/csv" \
    --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$TEST_GRAPH> { ?s ?p ?o } }" \
    2>/dev/null | tail -1 | tr -d '[:space:]'
}

setup_file() {
  local left
  left="$(_drop_test_graph)"
  if [ "${left:-0}" != "0" ]; then
    echo "FATAL: ${left} triples survived the pre-run clear of $TEST_GRAPH." >&2
    echo "This suite asserts a planted sibling SURVIVES a deploy; residue would" >&2
    echo "satisfy that assertion even if the deploy clobbered it." >&2
    exit 1
  fi
}

teardown() { _drop_test_graph >/dev/null; }
teardown_file() { _drop_test_graph >/dev/null; }

plant_sibling() {
  # mimic #3529: value-stream wiring loaded LIVE into the graph, NOT in any deployed TTL
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST -H 'Content-Type: application/sparql-update' --data-binary \
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
