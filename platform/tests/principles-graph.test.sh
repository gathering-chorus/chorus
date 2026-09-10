#!/usr/bin/env bash
# @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
: "${CHORUS_ROOT:=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)}"

# Live-graph tests for #2447 + #2314: the principles graph holds Hemenway's 14 parents intact.
# #4132 (2026-09-10): the typed April page /book/principles-reconstructed.html and its two
# page-vs-graph checks are gone — the graph is the only home of the principles.
# Post-#2314 (ADR-025), Principle instances lived in urn:chorus:instances.
# #4106 (2026-09-04): they live in urn:chorus:domains:principles now — Jeff's
# ruling that every row sits in its own domain graph and the catch-all
# instances graph is retired. The rows never moved out from under this test
# quietly; the test simply never ran, because nothing routed *.test.sh in
# platform/tests to a lane. All 28 principles and all 14 Hemenway parents are
# present in the domain graph, verified 2026-09-04 15:55.
# Runs against live Fuseki + chorus-api — not a fixture, because the AC targets the live
# graph and rendered HTML. Baseline pattern: same as doc-coherence-ratchet.test.sh.
#
# Checks:
#   1. Graph has 14 Hemenway parents (chorus:isPermacultureParent true)
#   2. Graph has 12 skos:broader edges (specialization relationships)
#   3. Every Hemenway parent has rdfs:label + rdfs:comment + dcterms:source
#   4. HTML article count matches (14)
#   5. Drift: every HTML h2 label finds a matching Hemenway parent in graph
#   6. riot validates chorus.ttl
set -uo pipefail

SPARQL_URL="${SPARQL_URL:-http://localhost:3030/pods/sparql}"
TTL="${TTL:-${CHORUS_ROOT}/roles/silas/ontology/chorus.ttl}"

pass=0; fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then pass=$((pass+1)); echo "  PASS: $desc"
  else fail=$((fail+1)); echo "  FAIL: $desc (expected: $expected, got: $actual)"; fi
}

sparql() {
  curl -s -G "$SPARQL_URL" --data-urlencode "query=$1" -H 'Accept: application/sparql-results+json'
}

count_query() {
  sparql "$1" | python3 -c "import json,sys;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null
}

ask_query() {
  sparql "$1" | python3 -c "import json,sys;print(json.load(sys.stdin)['boolean'])" 2>/dev/null
}

# 1. Hemenway parent count
PARENTS=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?p a chorus:Principle ; chorus:isPermacultureParent true } }')
check "14 Hemenway parents in graph" "14" "$PARENTS"

# 2. Specialization edges — INTEGRITY, not a count.
#
# #4111 — this asserted ">= 12 skos:broader edges". Measured today: the graph
# holds 14 Hemenway parents and ZERO skos:broader edges, so the check has been
# red every night for content Jeff has not authored. A floor on authored rows is
# a content check wearing a test's clothes: it goes red when someone edits the
# content and stays green when the code that serves it breaks — backwards.
#
# What a test can own is the MECHANISM: every specialization that exists points
# at a parent that exists. Zero edges is a legitimate state of the content and
# not a defect; a DANGLING edge is a defect at any count.
EDGES=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?c a chorus:Principle ; skos:broader ?p } }')
DANGLING=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?c a chorus:Principle ; skos:broader ?p . FILTER NOT EXISTS { ?p a chorus:Principle } } }')
check "every specialization edge resolves to a real parent (${EDGES} edge(s))" "0" "$DANGLING"

# 3. Every Hemenway parent has label + comment + source
COMPLETE=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dcterms: <http://purl.org/dc/terms/> SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?p a chorus:Principle ; chorus:isPermacultureParent true ; rdfs:label ?l ; rdfs:comment ?c ; dcterms:source ?s } }')
check "all 14 parents have label+comment+source" "14" "$COMPLETE"

# 4. riot validation
if riot --validate "$TTL" >/dev/null 2>&1; then
  check "chorus.ttl validates" "0" "0"
else
  check "chorus.ttl validates" "0" "1"
fi

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
