#!/bin/bash
# athena-validate — #3846. The post-write conformance sweep over the LIVE graph.
#
# The write door (athena-model) gates a single write: is THIS change conformant?
# It cannot see what's already there. athena-validate sweeps the whole instance
# graph for OLD / BAD data the door never gets to refuse:
#
#   1. RETIRED PREDICATES in use — edges the model retired (inParent, inProduct,
#      hostedBy, belongsTo, back-pointers). Present = stale data from before a
#      model change; the door 409s new ones but old ones linger.
#   2. DANGLING EDGES — an edge whose object IRI is not a subject anywhere: a
#      reference to a node that was deleted/renamed out from under it.
#   3. UNTYPED INSTANCES — a chorus: subject with predicates but no rdf:type:
#      data that exists but belongs to no class the model knows.
#
# Exit 0 = clean. Exit 1 = old/bad data found (report lists each). Read-only.
set -uo pipefail
FUSEKI="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
G="urn:chorus:instances"
NS="https://jeffbridwell.com/chorus#"
Q() { curl -sf --max-time 20 -H "Accept: application/sparql-results+json" --data-urlencode "query=$1" "$FUSEKI" 2>/dev/null; }
count() { echo "$1" | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["results"]["bindings"]))' 2>/dev/null || echo "?"; }
rows()  { echo "$1" | python3 -c 'import sys,json;[print("    "+" ".join(v["value"].split("#")[-1] for v in b.values())) for b in json.load(sys.stdin)["results"]["bindings"][:8]]' 2>/dev/null; }

BAD=0
echo "=== athena-validate — conformance sweep over GRAPH <$G> ==="

# 1. retired predicates still in use. This list is the model's OWN retired set
# (athena-product-design.html: 409 retired-predicate) — NOT a guess. inStream /
# inValueStream are CURRENT (steps use them), so they are deliberately absent:
# a check that flags valid predicates as bad is the #3850 hollow-check trap.
RETIRED="inParent inProduct hostedBy belongsTo"
echo "1) retired predicates in use:"
for p in $RETIRED; do
  r=$(Q "PREFIX c: <$NS> SELECT ?s WHERE { GRAPH <$G> { ?s c:$p ?o } } LIMIT 20")
  n=$(count "$r")
  if [ "$n" != "0" ] && [ "$n" != "?" ]; then BAD=$((BAD+n)); echo "  ⚠️  c:$p — $n subject(s)"; rows "$r"; fi
done
[ "$BAD" = "0" ] && echo "  ✅ none"

# 2. dangling edges — object is a chorus: IRI that is never a subject
echo "2) dangling edges (object node does not exist):"
DANGLE=$(Q "PREFIX c: <$NS> SELECT ?s ?p ?o WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(isIRI(?o) && STRSTARTS(STR(?o),\"$NS\")) FILTER NOT EXISTS { ?o ?anyp ?anyo } } } LIMIT 20")
nd=$(count "$DANGLE")
if [ "$nd" != "0" ] && [ "$nd" != "?" ]; then BAD=$((BAD+nd)); echo "  ⚠️  $nd dangling edge(s)"; rows "$DANGLE"; else echo "  ✅ none"; fi

# 3. untyped instances — a chorus: subject with data but no rdf:type
echo "3) untyped instances (data with no class):"
UNTYPED=$(Q "PREFIX c: <$NS> SELECT ?s WHERE { GRAPH <$G> { ?s ?p ?o . FILTER(STRSTARTS(STR(?s),\"$NS\")) FILTER NOT EXISTS { ?s a ?t } } } GROUP BY ?s LIMIT 20")
nu=$(count "$UNTYPED")
if [ "$nu" != "0" ] && [ "$nu" != "?" ]; then BAD=$((BAD+nu)); echo "  ⚠️  $nu untyped subject(s)"; rows "$UNTYPED"; else echo "  ✅ none"; fi

echo
if [ "$BAD" = "0" ]; then
  echo "PROVEN CLEAN — no old/bad data in the instance graph."
  exit 0
else
  echo "OLD/BAD DATA FOUND — $BAD issue(s). The write door can't reach these; this sweep is how they surface."
  exit 1
fi
