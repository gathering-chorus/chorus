#!/usr/bin/env bash

# Live-graph tests for #2447 + #2314: principles graph matches /book/principles-reconstructed.html.
# Post-#2314 (ADR-025), Principle instances live in urn:chorus:instances, not urn:chorus:ontology.
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
TTL="${TTL:-/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/ontology/chorus.ttl}"

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
PARENTS=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?p a chorus:Principle ; chorus:isPermacultureParent true } }')
check "14 Hemenway parents in graph" "14" "$PARENTS"

# 2. skos:broader edge count to Hemenway parents — grows over time as Jeff
# upstreams more specializations (#2471 multi-parent audit). Floor only.
EDGES=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?c a chorus:Principle ; skos:broader ?p . ?p chorus:isPermacultureParent true } }')
check ">=12 specialization edges (floor; grows)" "1" "$([ "$EDGES" -ge 12 ] && echo 1 || echo 0)"

# 3. Every Hemenway parent has label + comment + source
COMPLETE=$(count_query 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dcterms: <http://purl.org/dc/terms/> SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?p a chorus:Principle ; chorus:isPermacultureParent true ; rdfs:label ?l ; rdfs:comment ?c ; dcterms:source ?s } }')
check "all 14 parents have label+comment+source" "14" "$COMPLETE"

# 4. HTML article count
HTML_ARTICLES=$(curl -s "$HTML_URL" 2>/dev/null | grep -c '<article class="principle">')
check "HTML has 14 <article> elements" "14" "$HTML_ARTICLES"

# 5. Drift: HTML labels vs graph Hemenway parents
HTML_LABELS=$(curl -s "$HTML_URL" 2>/dev/null | grep -oE '<h2>[^<]*</h2>' | sed -E 's|<h2>[0-9]+\. *||; s|</h2>||' | head -14)
DRIFT=0
while IFS= read -r label; do
  [ -z "$label" ] && continue
  found=$(ask_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> ASK { GRAPH <urn:chorus:instances> { ?p a chorus:Principle ; chorus:isPermacultureParent true ; rdfs:label \"$label\" } }")
  [ "$found" != "True" ] && { DRIFT=$((DRIFT+1)); echo "    DRIFT: '$label' in HTML but not in graph"; }
done <<< "$HTML_LABELS"
check "0 label drift between HTML and graph" "0" "$DRIFT"

# 6. riot validation
if riot --validate "$TTL" >/dev/null 2>&1; then
  check "chorus.ttl validates" "0" "0"
else
  check "chorus.ttl validates" "0" "1"
fi

echo ""
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
