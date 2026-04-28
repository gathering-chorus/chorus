#!/usr/bin/env bash

# Live-graph drift test for #2485 Move 3: decisions graph matches expected baseline.
# Post-#2485 Move 1, Decision instances live in urn:chorus:instances with chorus:contains
# edges from loom-decisions subdomain. Baseline counts derived from canonical sources:
#   - 114 DECs in roles/wren/decisions.md
#   - 26 ADRs in roles/*/adr/ADR-*.md
#   - 140 total chorus:Decision instances
#   - 140 chorus:contains edges from loom-decisions
#
# This test fails CI if:
#   - Source markdown is edited but graph is stale (counts diverge)
#   - Graph instances created/deleted without updating canonical sources
#   - ADR-026 (the demo target) breaks
#
# Pattern: mirrors principles-graph.test.sh (#2447). Runs against live Fuseki — not fixtures —
# because the AC targets the live graph.

set -uo pipefail

SPARQL_URL="${SPARQL_URL:-http://localhost:3030/pods/sparql}"
API_URL="${API_URL:-http://localhost:3340}"
DECISIONS_MD="${DECISIONS_MD:-/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/decisions.md}"
ADR_GLOB="${ADR_GLOB:-/Users/jeffbridwell/CascadeProjects/chorus/roles/*/adr/ADR-*.md}"

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

literal_query() {
  sparql "$1" | python3 -c "import json,sys;b=json.load(sys.stdin)['results']['bindings'];print(b[0]['v']['value'] if b else '')" 2>/dev/null
}

echo "=== decisions-graph drift test (#2485 Move 3) ==="

# 1. Source counts
DEC_COUNT=$(grep -cE '^## DEC-[0-9]+' "$DECISIONS_MD")
# shellcheck disable=SC2086
ADR_COUNT=$(ls $ADR_GLOB 2>/dev/null | wc -l | tr -d ' ')
TOTAL_SOURCE=$((DEC_COUNT + ADR_COUNT))

# 2. Graph counts
GRAPH_DEC_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision } }")
CONTAINS_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#loom-decisions> chorus:contains ?d . ?d a chorus:Decision } }")

check "Total source count = total graph instance count" "$TOTAL_SOURCE" "$GRAPH_DEC_COUNT"
check "All instances have chorus:contains edge from loom-decisions" "$GRAPH_DEC_COUNT" "$CONTAINS_COUNT"

# 3. Each Decision has required predicates (id, label, comment, decisionType)
ID_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision ; chorus:id ?id } }")
LABEL_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision ; rdfs:label ?l } }")
TYPE_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision ; chorus:decisionType ?t } }")

check "All instances have chorus:id" "$GRAPH_DEC_COUNT" "$ID_COUNT"
check "All instances have rdfs:label" "$GRAPH_DEC_COUNT" "$LABEL_COUNT"
check "All instances have chorus:decisionType" "$GRAPH_DEC_COUNT" "$TYPE_COUNT"

# 4. ADR-026 (demo target) retrievable with full predicate set
ADR026_LABEL=$(literal_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?v WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#adr-026> rdfs:label ?v } }")
ADR026_STATUS=$(literal_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#adr-026> chorus:status ?v } }")
ADR026_CARD=$(literal_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?v WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#adr-026> chorus:relatedCard ?v } }")

check "ADR-026 label present" "CI architecture + lock-file policy" "$ADR026_LABEL"
check "ADR-026 status = Accepted" "Accepted" "$ADR026_STATUS"
check "ADR-026 relatedCard = 2481" "2481" "$ADR026_CARD"

# 5. Type discrimination: ADR + DEC counts split correctly
ADR_GRAPH_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision ; chorus:decisionType \"ADR\" } }")
DEC_GRAPH_COUNT=$(count_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?d) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?d a chorus:Decision ; chorus:decisionType \"DEC\" } }")

check "ADR count in graph = ADR file count" "$ADR_COUNT" "$ADR_GRAPH_COUNT"
check "DEC count in graph = DEC source count" "$DEC_COUNT" "$DEC_GRAPH_COUNT"

# 6. API endpoint round-trip: /api/athena/subdomains/loom-decisions/decisions returns expected count
API_COUNT=$(curl -s "$API_URL/api/athena/subdomains/loom-decisions/decisions" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('_meta',{}).get('count','?'))" 2>/dev/null)
check "API endpoint count = graph instance count" "$GRAPH_DEC_COUNT" "$API_COUNT"

# 7. Loom redirect: /api/loom/decisions returns 308
LOOM_REDIRECT=$(curl -s -o /dev/null -w "%{http_code}" "$API_URL/api/loom/decisions")
check "/api/loom/decisions returns 308" "308" "$LOOM_REDIRECT"

# 8. /loom/decisions.html page renders (closing AC: page mirrors /loom/principles.html)
PAGE_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$API_URL/loom/decisions.html")
check "/loom/decisions.html returns 200" "200" "$PAGE_STATUS"

# 9. Closing AC demo target: ADR-026 §a-d retrievable via API (pre-MCP probe).
# This asserts the demo path is real before Move 5's chorus_decisions_get tool ships.
ADR026_BODY_LEN=$(curl -s "$API_URL/api/athena/subdomains/loom-decisions/decisions" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for x in d.get('data',{}).get('decisions',[]):
    if x.get('id') == 'adr-026':
        print(len(x.get('comment','')))
        break
" 2>/dev/null)
# ADR-026 body is multi-kilobyte (full ADR text); assert >= 2000 chars to confirm body fidelity.
if [ -n "$ADR026_BODY_LEN" ] && [ "$ADR026_BODY_LEN" -ge 2000 ]; then
  pass=$((pass+1)); echo "  PASS: ADR-026 body retrievable via API (${ADR026_BODY_LEN} chars)"
else
  fail=$((fail+1)); echo "  FAIL: ADR-026 body too short or missing (got: ${ADR026_BODY_LEN:-empty})"
fi

echo
echo "=== summary: $pass pass, $fail fail ==="
[ "$fail" -eq 0 ]
