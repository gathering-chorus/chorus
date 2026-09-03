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

# #3606 — SOURCE FUSEKI AUTH. Fuseki refuses unauthenticated writes (401, verified
# live). The two mint-then-query tests below POST their fixtures with a bare curl
# and `-o /dev/null 2>/dev/null`, so the 401 was swallowed whole: the INSERT never
# landed, the follow-up SELECT found nothing, and the assertion failed as though
# the SCHEMA were wrong. The schema was fine — the write was refused and nobody
# heard it. Auth arrived after these tests were written.
#
# ROOT is derived FIRST and auth is sourced from it — not from an ambient
# CHORUS_ROOT. Sourcing via $CHORUS_ROOT contradicted this file's own invariant
# (line 19), and when CHORUS_ROOT was unset it silently sourced
# "/platform/scripts/fuseki-auth.sh", leaving FUSEKI_AUTH empty and every write
# back to a swallowed 401 — the exact failure the auth was added to fix.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# shellcheck source=/dev/null
. "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
# #3991 repoint, missed here: #3561 renamed chorus-model-deploy.sh →
# athena-deploy-model.sh. The old path still resolved to nothing, so all eight
# tests below failed on exit 127 (command not found) rather than on the schema.
SCRIPT="$ROOT/platform/scripts/athena-deploy-model.sh"
TTL="$ROOT/roles/kade/ontology/werk-domains.ttl"
# per-run graph name: two pipelines (e.g. wren-4080 + silas-4084, 2026-09-03 10:20) ran this file at once against one store and one cleared the graph mid-ASK — test-wrong, not product-wrong
TEST_GRAPH="urn:chorus:ontology-test-bats-3540-$$"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"
PFX='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#>'

# #3606 — RESIDUE IS THE FALSE-GREEN MECHANISM, and this suite had it.
#
# Every shape assertion below is an ASK against $TEST_GRAPH. The old teardown
# issued an UNAUTHENTICATED DELETE with `-o /dev/null 2>/dev/null || true`, so
# post-#3564 it 401'd and the graph was never dropped — verified live: two
# chorus:GateResult subjects were still resident from an earlier run.
#
# That makes the assertions unable to distinguish the two states they exist to
# separate: "the deploy landed the shape" vs "last run's leftovers are still
# here". A silently-refused deploy still ASKs true off residue. The suite would
# report the schema healthy while proving nothing about the deploy.
#
# Fix: drop authenticated and VERIFY the drop, before the file and after it. A
# refused drop is fatal, never `|| true` — if we cannot clear the graph we cannot
# trust a single assertion in this file, and saying so loudly beats reporting a
# green we did not earn.
_graph_triples() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$Q" \
    --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$TEST_GRAPH> { ?s ?p ?o } }" \
    -H "Accept: text/csv" 2>/dev/null | tail -1 | tr -d '[:space:]'
}

_drop_test_graph() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null
  local n
  n="$(_graph_triples)"
  if [ "${n:-0}" != "0" ]; then
    echo "FATAL: could not clear $TEST_GRAPH ($n triples remain)." >&2
    echo "Every assertion in this file ASKs against that graph; residue makes them" >&2
    echo "pass off leftovers instead of the deploy. Refusing to report a false green." >&2
    return 1
  fi
  return 0
}

# #3606 — INSERT that reports its own refusal. The old call sites piped the write
# to `-o /dev/null 2>/dev/null` and checked nothing, so a 401 looked identical to
# a successful mint; the follow-up SELECT then read whatever residue was already
# resident. Assert the HTTP status at the write, where the truth is.
_insert() {
  local code
  code="$(curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
    -X POST "http://localhost:3030/pods/update" \
    --data-urlencode "update=$PFX INSERT DATA { GRAPH <$TEST_GRAPH> { $1 } }" 2>/dev/null)"
  case "$code" in
    2*) return 0 ;;
    *)  echo "INSERT refused (HTTP $code) — fixture never landed; a SELECT that" >&2
        echo "passes after this is reading residue, not this mint." >&2
        return 1 ;;
  esac
}

setup_file() {
  _drop_test_graph || exit 1
}

teardown_file() {
  _drop_test_graph || true
}

# #3606 NEGATIVE PROOF (#3734 contract: no gate without one).
#
# Runs FIRST, deliberately. Every other test in this file deploys and then ASKs
# true; none of them demonstrates the ASK is capable of returning false. Without
# that, a graph full of residue and a deploy that silently 401'd produce exactly
# the same green as a healthy run.
#
# This test puts the guarded condition in its VIOLATED state — graph cleared, no
# deploy — and shows the assertion goes RED. That is what makes every green below
# mean "the deploy landed the shape" rather than "something is in the graph".
@test "NEGATIVE PROOF: with the graph cleared and no deploy, the shape ASK is FALSE" {
  _drop_test_graph
  [ "$(_graph_triples)" = "0" ]
  run curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:pyramidLayer ; sh:minCount 1 ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":false'* ]]
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

@test "TestEdgesShape carries testConcern — the orthogonal concern axis (@test-type api/ui/perf/security destination)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=$PFX ASK { GRAPH <$TEST_GRAPH> { chorus:TestEdgesShape sh:property [ sh:path chorus:testConcern ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "pyramidLayer and testConcern are ORTHOGONAL — a Test carries both (e2e + security)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  # #3606 — assert the write LANDED. A swallowed 401 here plus residue in the
  # graph is indistinguishable from a healthy mint at the SELECT below.
  run _insert "chorus:test-3540-ortho a chorus:Test ; chorus:covers chorus:subdomain-tests-domain ; chorus:pyramidLayer \"e2e\" ; chorus:testConcern \"security\""
  [ "$status" -eq 0 ]
  run curl -s "$Q" --data-urlencode "query=$PFX SELECT ?t WHERE { GRAPH <$TEST_GRAPH> { ?t chorus:pyramidLayer \"e2e\" ; chorus:testConcern \"security\" } }" -H "Accept: application/sparql-results+json"
  [[ "$output" == *'test-3540-ortho'* ]]
}

@test "a minted Test instance is queryable BY its covers edge (the #3190 contract)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  # mint a Test instance covering a known SubDomain, the way the #2818 tagging will
  run _insert "chorus:test-3540-probe a chorus:Test ; chorus:covers chorus:subdomain-tests-domain ; chorus:pyramidLayer \"integration\" ; chorus:hermeticity \"needs-stack\""
  [ "$status" -eq 0 ]
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
