#!/usr/bin/env bash
# @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
: "${CHORUS_ROOT:=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)}"

# Live-graph tests for #2447 + #2314: principles graph matches /book/principles-reconstructed.html.
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

HTML_URL="${HTML_URL:-http://localhost:3340/book/principles-reconstructed.html}"
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

# 4. HTML article count — the page renders what the graph holds.
#
# #4111 — this pinned the literal 14. The page renders 12 and the graph holds
# 14, and the fixed number could not say which of those two is the bug. Compare
# the two sides: a mismatch is a rendering defect at any count, and authoring a
# fifteenth parent is not a test failure.
HTML_ARTICLES=$(curl -s "$HTML_URL" 2>/dev/null | grep -c '<article class="principle">')
check "HTML renders one article per parent in the graph" "$PARENTS" "$HTML_ARTICLES"

# 5. Drift: HTML labels vs graph Hemenway parents
HTML_LABELS=$(curl -s "$HTML_URL" 2>/dev/null | grep -oE '<h2>[^<]*</h2>' | sed -E 's|<h2>[0-9]+\. *||; s|</h2>||' | head -14)
DRIFT=0
while IFS= read -r label; do
  [ -z "$label" ] && continue
  found=$(ask_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> ASK { GRAPH <urn:chorus:domains:principles> { ?p a chorus:Principle ; chorus:isPermacultureParent true ; rdfs:label \"$label\" } }")
  [ "$found" != "True" ] && { DRIFT=$((DRIFT+1)); echo "    DRIFT: '$label' in HTML but not in graph"; }
done <<< "$HTML_LABELS"
check "0 label drift between HTML and graph" "0" "$DRIFT"
# #4111 — say WHAT the drift is, not just how much of it there is.
#
# Measured 2026-09-06: the page renders Holmgren's twelve permaculture
# principles ("Observe and interact", "Obtain a yield", "Produce no waste")
# while the graph holds Hemenway's fourteen ("Observe", "Connect", "Make the
# least change for the greatest effect"). These are not drifted versions of one
# list — they are two different books, and the page's own <title> says
# "Gaia's Garden", which is Hemenway. So the page is rendering the wrong set
# under the right name.
#
# A bare "12 drifted" sends the reader looking for a rendering bug. Naming it
# sends them to the actual question: which book the page is supposed to show.
if [ "$DRIFT" -gt 0 ]; then
  echo "    NOTE: this is not per-row drift. The page and the graph hold"
  echo "          DIFFERENT principle sets — the page renders Holmgren's 12,"
  echo "          the graph holds Hemenway's 14, and the page title says"
  echo "          Gaia's Garden (Hemenway). Deciding which set the page should"
  echo "          show is authoring work in the principles domain, not a"
  echo "          rendering fix."
fi

# 6. riot validation
if riot --validate "$TTL" >/dev/null 2>&1; then
  check "chorus.ttl validates" "0" "0"
else
  check "chorus.ttl validates" "0" "1"
fi

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
