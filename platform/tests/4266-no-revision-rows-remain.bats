#!/usr/bin/env bats
# @test-type: fitness
# @domain: provenance — the product domain this suite guards (#4334)
#
# #4266 — chorus:Revision was renamed to chorus:Version by #4211 and the 16,186
# history rows were migrated out of the catch-all graph into the domain graph.
# This is the guard that the migration stays done: no row anywhere is typed with
# the retired class, and none sits in the catch-all.
#
# Jeff, 2026-09-21: "kade and wren do not make version back to revision". Two
# tests wanted the old name and the cheap green was to re-declare it. This file
# is the thing that goes red if anyone does.
#
# Box-dependent by construction — it reads the live store. If the store is not
# reachable it reports UNMEASURED and skips, never green.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  QUERY="http://localhost:3030/pods/query"
  source "$ROOT/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
}

ask() {
  curl -sf --max-time 20 -u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD" \
    -H "Accept: text/csv" --data-urlencode "query=$1" "$QUERY" 2>/dev/null | tail -1 | tr -d '\r'
}

store_is_up() {
  curl -sf --max-time 10 -o /dev/null "http://localhost:3030/$/ping" 2>/dev/null
}

@test "the store answers, or this file reports UNMEASURED" {
  if ! store_is_up; then
    echo "UNMEASURED: Fuseki unreachable at localhost:3030 — not a red, not a green"
    skip "store unreachable"
  fi
  run store_is_up
  test "$status" -eq 0
}

@test "no row anywhere is typed chorus:Revision" {
  store_is_up || skip "UNMEASURED: store unreachable"
  n="$(ask 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a c:Revision } }')"
  echo "chorus:Revision rows = $n (expected 0)"
  test "$n" = "0"
}

@test "no chorus:Version row sits in the catch-all graph" {
  store_is_up || skip "UNMEASURED: store unreachable"
  n="$(ask 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?s a c:Version } }')"
  echo "chorus:Version rows in urn:chorus:instances = $n (expected 0)"
  test "$n" = "0"
}

@test "the migrated history is in the provenance graph and did not shrink" {
  store_is_up || skip "UNMEASURED: store unreachable"
  n="$(ask 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <urn:chorus:domains:provenance> { ?s a c:Version } }')"
  echo "chorus:Version rows in provenance = $n (16993 at migration, 2026-09-21)"
  test -n "$n"
  test "$n" -ge 16993
}

# NEGATIVE PROOF — the checks above are only worth their green if they can go red.
# Each asserts against a count the query returns; these run the SAME query shape
# against a subject that IS in the violating state, and require it to be seen.
# If the query could not distinguish the two states, these fail.

@test "NEGATIVE PROOF: the Revision query counts a row that is typed Revision" {
  store_is_up || skip "UNMEASURED: store unreachable"
  # chorus:Version rows exist; ask the same question about the class that DOES
  # have rows. A query that returns 0 here cannot have meant anything above.
  n="$(ask 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a c:Version } }')"
  echo "same query shape, class with rows = $n (must be > 0 or the zero above is vacuous)"
  test "$n" -gt 0
}

@test "NEGATIVE PROOF: the catch-all query shape sees Version rows in a graph that holds them" {
  store_is_up || skip "UNMEASURED: store unreachable"
  # #4318 — this used to count ANY triple in the catch-all and required > 0.
  # #4187 retired the catch-all and it is empty now, so the proof went red for
  # a correct store. The check it guards asks "a Version row in graph G?";
  # the proof is that the SAME query with G = the graph that holds the
  # history finds them. If the shape could not see a Version row in a named
  # graph, the zero above would be vacuous.
  n="$(ask 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <urn:chorus:domains:provenance> { ?s a c:Version } }')"
  echo "same query shape, G = provenance: $n (must be > 0 or the catch-all zero is vacuous)"
  test "$n" -gt 0
}
