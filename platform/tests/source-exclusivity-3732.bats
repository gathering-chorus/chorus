#!/usr/bin/env bats
# @test-type: integration — queries the live store; RUN_INTEGRATION-gated (TEST.md two-mode contract)
load test_helper
# source-exclusivity-3732.bats — ADR-051 Addendum II, the enforcing check.
#
# The rule: a shape (and an instance class) has ONE source of truth. Two graphs
# defining the same shape is not redundancy — it is a system with no fact of the
# matter about which definition governs. That produced four wrong /products
# diagnoses in eight weeks; #3732 found the schema-layer instance
# (urn:chorus:shapes' 3-property ProductShape against the ontology's live 11).
#
# NEGATIVE PROOF (#3734): the last test constructs the violation in a scratch
# graph pair and shows the check FAILS on it. Without that, a green run here
# proves only that the query ran.

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live Fuseki) — RUN_INTEGRATION=true to run"
  source "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh"
  Q="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
  GSP="${FUSEKI_GSP:-http://localhost:3030/pods/data}"
}

# Count graphs defining a given shape. Empty answer = fail-closed (#3731).
shape_graph_count() {
  local shape="$1" resp
  resp=$(curl -s "$Q" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { <https://jeffbridwell.com/chorus#$shape> <http://www.w3.org/ns/shacl#property> ?p } }" \
    -H 'Accept: text/csv')
  printf '%s' "$resp" | head -1 | grep -q '^n' || { echo "UNANSWERED"; return; }
  printf '%s\n' "$resp" | tail -1 | tr -dc '0-9'
}

@test "no SHACL shape is defined in more than one graph (source exclusivity)" {
  # Every shape the store knows, checked — not a hand-picked list, or the check
  # only covers what someone remembered to name.
  resp=$(curl -s "$Q" --data-urlencode \
    'query=SELECT ?s (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { ?s a <http://www.w3.org/ns/shacl#NodeShape> } } GROUP BY ?s HAVING (COUNT(DISTINCT ?g) > 1)' \
    -H 'Accept: text/csv')
  echo "multi-graph shapes: $resp"
  [ -n "$resp" ]                       # unanswered store = fail, never silent pass
  printf '%s' "$resp" | head -1 | grep -q '^s'
  # header only = zero violations
  [ "$(printf '%s\n' "$resp" | grep -c .)" -le 1 ]
}

@test "ProductShape specifically has exactly one home (the #3732 case)" {
  n=$(shape_graph_count ProductShape)
  echo "ProductShape graphs: $n"
  [ "$n" != "UNANSWERED" ]
  [ "$n" = "1" ]
}

@test "NEGATIVE PROOF: a shape defined in TWO graphs is DETECTED by this check" {
  A="urn:chorus:excl-test-a-$$"; B="urn:chorus:excl-test-b-$$"
  TT="$BATS_TEST_TMPDIR/dup.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\nchorus:ExclDupShape a sh:NodeShape ; sh:property [ sh:path chorus:x ] .\n' > "$TT"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' --data-binary @"$TT" "$GSP?graph=$A" -o /dev/null
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' --data-binary @"$TT" "$GSP?graph=$B" -o /dev/null
  n=$(shape_graph_count ExclDupShape)
  # cleanup before asserting, so a failure doesn't leave fixtures behind
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$A" -o /dev/null
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$B" -o /dev/null
  echo "duplicate shape seen in $n graphs (must be 2 — the check can SEE the violation)"
  [ "$n" = "2" ]
}
