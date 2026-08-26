#!/usr/bin/env bats
# @test-type: integration
#
# #4010 — the design of a thing lives IN THE GRAPH, not in a file that quotes it.
#
# Jeff, 2026-08-26: "im confused about why we made html when i asked to put in
# the graph." He was right. The first pass wrote two HTML files carrying a
# generated identity block and called it done, while /products/pulse still
# answered hasDesignDoc ABSENT and /documents 404'd.
#
# THE ROOT, and the reason this sat for months: DocumentShape required
# `hasDomain` of class chorus:SubDomain — a level #3509 re-levelled away. The
# shape asked for something no live instance can be, so the set stayed empty and
# every product kept the text "hasDesignDoc unfilled (no Document instances in
# graph)" in its gaps field. An unsatisfiable shape fails silently; it just
# produces nothing forever.

NS="https://jeffbridwell.com/chorus#"
EP="http://localhost:3030/pods/sparql"
GSP="http://localhost:3030/pods/data"
DOCS="urn:chorus:domains:documents"
FIXTURE="urn:chorus:test:documents-4010-fixture"

count() { curl -s --max-time 10 "$EP" --data-urlencode \
  "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH <$1> { $2 } }" \
  -H "Accept: application/sparql-results+json" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])"; }

teardown() {
  # shellcheck disable=SC1091
  source "${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$FIXTURE" -o /dev/null 2>/dev/null || true
}

@test "the Pulse designs exist as Document instances" {
  run count "$DOCS" '?s a chorus:Document'
  [ "$output" -ge 2 ]
}

@test "every Document carries a title and an href — a row nobody can open is not a document" {
  run count "$DOCS" '?s a chorus:Document FILTER NOT EXISTS { ?s chorus:docTitle ?t }'
  [ "$output" = "0" ]
  run count "$DOCS" '?s a chorus:Document FILTER NOT EXISTS { ?s chorus:docHref ?h }'
  [ "$output" = "0" ]
}

@test "hasDesignDoc on chorus:pulse RESOLVES — the ProductShape floor is satisfied" {
  # The whole point. Before #4010 this returned nothing on every product in the
  # model, and the gaps field said so in prose instead of the shape saying it in
  # data.
  n=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?d) AS ?c) WHERE { GRAPH ?g { chorus:pulse chorus:hasDesignDoc ?d } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$n" -ge 2 ]
}

@test "the shape no longer demands a retired class" {
  # The regression guard: if anyone re-points DocumentShape at SubDomain, the set
  # becomes unsatisfiable again and this goes red rather than quietly emptying.
  # Scoped to DocumentShape's own block. Three OTHER shapes still reference
  # SubDomain (chorus.ttl:1970, 1975, 3690) — real debt, not this card's, and a
  # whole-file grep would make this test fail for someone else's reason.
  src="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)/roles/silas/ontology/chorus.ttl"
  # awk has no \s — an unterminated range runs to EOF and picks up the OTHER
  # shapes' SubDomain lines, which is how this guard first went red for the
  # wrong reason.
  block=$(awk '/^chorus:DocumentShape a sh:NodeShape/,/chorus:generatedBy/' "$src")
  [ -n "$block" ]
  case "$block" in *"sh:class chorus:Domain"*) : ;; *) echo "DocumentShape lost its Domain range"; false ;; esac
  case "$block" in *"sh:class chorus:SubDomain"*) echo "DocumentShape re-points at retired SubDomain"; false ;; *) : ;; esac
}

# NEGATIVE PROOF 1 — the href gate must RED on a Document with no docHref. The
# deploy asks this same question against staging; if the query cannot see the
# state it exists to refuse, every "gate passed" line it prints is meaningless.
@test "the href gate REDS on a Document with no docHref" {
  # shellcheck disable=SC1091
  source "${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  code=$(printf '%s\n' \
    '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
    '@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .' \
    'chorus:doc-fixture-4010 a chorus:Document ;' \
    '    rdfs:label "Fixture" ;' \
    '    chorus:docTitle "Fixture with no href" .' \
    | curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
        -X POST -H 'Content-Type: text/turtle' --data-binary @- "$GSP?graph=$FIXTURE")
  [ "$code" = "200" ] || [ "$code" = "201" ] || [ "$code" = "204" ]

  run count "$FIXTURE" '?s a chorus:Document'
  [ "$output" = "1" ]

  run count "$FIXTURE" '?s a chorus:Document FILTER NOT EXISTS { ?s chorus:docHref ?h }'
  [ "$output" = "1" ]
}

# NEGATIVE PROOF 2 — the SAME query reads 0 over the real graph, so the check
# separates "hrefless" from "fine" rather than reporting one of them always.
@test "the same query reads 0 over the real documents graph" {
  run count "$DOCS" '?s a chorus:Document FILTER NOT EXISTS { ?s chorus:docHref ?h }'
  [ "$output" = "0" ]
}
