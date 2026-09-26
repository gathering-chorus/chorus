#!/usr/bin/env bats
# @test-type: integration
#
# #4006 — the loom's value/principle layer, graded against the STORE and against
# the deploy's own refusals. Two halves, and the second is the one that matters:
#
#  1. WHAT IS SERVED. The principles are CONSISTENT, not a fixed number: the
#     14 Hemenway parents (pc) are a source fact and stay counted; total must
#     equal pc + xp; every one carries principleKind; no probe row (zz-) is
#     live. A literal 28 went red the day a principle was added (#4273) — a
#     test that reds when Jeff adds a principle is backwards. 5 values, each
#     with at least one expressedBy edge.
#  2. WHAT IS REFUSED. The dangling-edge gate, now in athena-deploy —
#     run against a fixture value whose expressedBy points at an IRI no
#     Principle occupies, it must EXIT NON-ZERO. Without that case the gate is
#     a line of shell nobody has ever seen fail, which is the hollow-gate shape
#     #3734 forbids.
#
# The fixture never touches a served graph: it loads into a throwaway staging
# graph, is asked there, and is deleted in teardown whatever the result.

NS="https://jeffbridwell.com/chorus#"
EP="http://localhost:3030/pods/sparql"
GSP="http://localhost:3030/pods/data"
PRIN_GRAPH="urn:chorus:domains:principles"
VAL_GRAPH="urn:chorus:domains:values"
FIXTURE_GRAPH="urn:chorus:test:values-4006-fixture"
# #4332 — the fixture goes to the test dataset, never /pods (lib/test-store.sh).
# The consistency checks below still READ the real graphs on /pods.
. "$BATS_TEST_DIRNAME/lib/test-store.sh"

count() { # $1 = graph, $2 = WHERE body
  curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?s) AS ?c) WHERE { GRAPH <$1> { $2 } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])"
}

teardown() {
  # shellcheck disable=SC1091
  source "${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  if [ -n "${TEST_STORE:-}" ]; then
    curl -s --max-time 30 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$TEST_STORE/data?graph=$FIXTURE_GRAPH" -o /dev/null 2>/dev/null || true
    curl -s --max-time 30 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$TEST_STORE/data?graph=$PRIN_GRAPH" -o /dev/null 2>/dev/null || true
  fi
}

@test "principles are consistent: 14 Hemenway parents, total == pc + xp, no probe row" {
  run count "$PRIN_GRAPH" '?s a chorus:Principle'
  total="$output"

  run count "$PRIN_GRAPH" '?s a chorus:Principle ; chorus:principleKind "pc"'
  [ "$output" = "14" ]
  pc="$output"

  run count "$PRIN_GRAPH" '?s a chorus:Principle ; chorus:principleKind "xp"'
  xp="$output"
  [ "$total" -gt 0 ]
  [ "$total" = "$((pc + xp))" ]

  # A probe row left behind by a hand test (principle-zz-generated-principle-probe,
  # 2026-09-21) sat in the served graph for two days; this count read 1 then, 0 now.
  run count "$PRIN_GRAPH" '?s a chorus:Principle FILTER(CONTAINS(STR(?s), "zz-") || EXISTS { ?s <http://www.w3.org/2000/01/rdf-schema#label> ?l FILTER(STRSTARTS(STR(?l), "zz-")) })'
  [ "$output" = "0" ]
}

@test "every principle carries a kind — none reads blank" {
  # The defect this replaces: 14 served with principleKind on ZERO, rendering an
  # empty axis rather than refusing. Asked as ABSENCE, which is the half a
  # count-the-tagged check cannot see.
  run count "$PRIN_GRAPH" '?s a chorus:Principle FILTER NOT EXISTS { ?s chorus:principleKind ?k }'
  [ "$output" = "0" ]
}

@test "5 values are served and each expresses at least one principle" {
  run count "$VAL_GRAPH" '?s a chorus:Value'
  [ "$output" = "5" ]

  run count "$VAL_GRAPH" '?s a chorus:Value FILTER NOT EXISTS { ?s chorus:expressedBy ?p }'
  [ "$output" = "0" ]
}

@test "every expressedBy target is a live Principle — no edge to nothing" {
  target_missing=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?t) AS ?c) WHERE { GRAPH <$VAL_GRAPH> { ?v chorus:expressedBy ?t } FILTER NOT EXISTS { GRAPH <$PRIN_GRAPH> { ?t a chorus:Principle } } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$target_missing" = "0" ]
}

# NEGATIVE PROOF — the dangling-edge condition must be DETECTABLE. A fixture
# value pointing at an IRI no Principle occupies is loaded to a throwaway graph
# and the gate's own query is asked against it: it must report 1, not 0. If this
# reads 0, the query in athena-deploy cannot see the state it exists to
# refuse, and every "gate passed" line it prints is meaningless.
@test "the dangling-edge query REDS on a value pointing at a non-principle" {
  # The write door needs the #3566 credential — a bare POST 401s, and a test that
  # treats 401 as "loaded" would then ask an EMPTY graph and read 0 dangling,
  # passing for the wrong reason. So the load is asserted before the query runs.
  # shellcheck disable=SC1091
  source "${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  # #4332 — the whole negative proof runs in the test dataset: one principle
  # that exists, one value pointing at it, one value pointing at nothing.
  # The query must count exactly the dangling one.
  test_store || skip "UNMEASURED: $TEST_STORE_WHY"
  GSP="$FUSEKI_GSP"; EP="$FUSEKI_QUERY"
  assert_not_prod "$GSP"
  code=$(printf '%s\n' \
    '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
    'chorus:xp-real-principle-4006 a chorus:Principle .' \
    | curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
        -X PUT -H 'Content-Type: text/turtle' --data-binary @- "$GSP?graph=$PRIN_GRAPH")
  [ "$code" = "200" ] || [ "$code" = "201" ] || [ "$code" = "204" ]
  code=$(printf '%s\n' \
    '@prefix chorus: <https://jeffbridwell.com/chorus#> .' \
    '@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .' \
    'chorus:xp-value-fixture-4006 a chorus:Value ;' \
    '    rdfs:label "Fixture" ;' \
    '    rdfs:comment "Points at a principle that does not exist." ;' \
    '    chorus:expressedBy chorus:xp-no-such-principle-4006 .' \
    'chorus:xp-value-fixture-ok-4006 a chorus:Value ;' \
    '    rdfs:label "Fixture ok" ;' \
    '    chorus:expressedBy chorus:xp-real-principle-4006 .' \
    | curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
        -X PUT -H 'Content-Type: text/turtle' --data-binary @- "$GSP?graph=$FIXTURE_GRAPH")
  [ "$code" = "200" ] || [ "$code" = "201" ] || [ "$code" = "204" ]

  # And the fixture really is in there — a 2xx on an empty body would still
  # leave the graph empty, and an empty graph reads 0 dangling.
  run count "$FIXTURE_GRAPH" '?s a chorus:Value'
  [ "$output" = "2" ]

  dangling=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?t) AS ?c) WHERE { GRAPH <$FIXTURE_GRAPH> { ?v chorus:expressedBy ?t } FILTER NOT EXISTS { GRAPH <$PRIN_GRAPH> { ?t a chorus:Principle } } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$dangling" = "1" ]
}

# And the mirror: the SAME query over the real values graph reports 0, so the
# check separates "dangling" from "fine" rather than reporting one of them
# always. A check that says 1 everywhere is as useless as one that says 0.
@test "the same query reads 0 over the real values — it separates the two states" {
  real=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?t) AS ?c) WHERE { GRAPH <$VAL_GRAPH> { ?v chorus:expressedBy ?t } FILTER NOT EXISTS { GRAPH <$PRIN_GRAPH> { ?t a chorus:Principle } } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$real" = "0" ]
}

@test "each value expresses at least one PC and at least one XP principle" {
  # Jeff's mapping 2026-08-25. A value with feet in only one lineage reads as
  # belonging to that lineage; both-or-nothing is the assertion.
  for v in communication simplicity feedback courage respect; do
    pc=$(curl -s --max-time 10 "$EP" --data-urlencode \
      "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?p) AS ?c) WHERE { GRAPH <$VAL_GRAPH> { chorus:xp-value-$v chorus:expressedBy ?p } GRAPH <$PRIN_GRAPH> { ?p chorus:principleKind \"pc\" } }" \
      -H "Accept: application/sparql-results+json" \
      | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
    xp=$(curl -s --max-time 10 "$EP" --data-urlencode \
      "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?p) AS ?c) WHERE { GRAPH <$VAL_GRAPH> { chorus:xp-value-$v chorus:expressedBy ?p } GRAPH <$PRIN_GRAPH> { ?p chorus:principleKind \"xp\" } }" \
      -H "Accept: application/sparql-results+json" \
      | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
    [ "$pc" -ge 1 ] || { echo "$v has no PC principle"; false; }
    [ "$xp" -ge 1 ] || { echo "$v has no XP principle"; false; }
  done
}

@test "the PC-XP rhyme is asymmetric: 11 bound, 3 XP deliberately unbound" {
  # The EMPTY half is the assertion. humanity, quality and accepted-responsibility
  # have no permaculture twin, and a later pass that invents one for the sake of
  # a tidy table must fail here rather than pass quietly.
  bound=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(DISTINCT ?x) AS ?c) WHERE { GRAPH <$PRIN_GRAPH> { ?x chorus:rhymesWith ?p } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$bound" = "11" ]

  for u in xp-humanity xp-quality xp-accepted-responsibility; do
    n=$(curl -s --max-time 10 "$EP" --data-urlencode \
      "query=PREFIX chorus: <$NS> SELECT (COUNT(?p) AS ?c) WHERE { GRAPH <$PRIN_GRAPH> { chorus:$u chorus:rhymesWith ?p } }" \
      -H "Accept: application/sparql-results+json" \
      | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
    [ "$n" = "0" ] || { echo "$u was given a rhyme it does not have"; false; }
  done
}

@test "every rhyme joins a pc to an xp — never within one lineage" {
  # A rhyme is a bridge between vocabularies. Two PC principles rhyming with
  # each other, or two XP, would be a synonym claim inside one lineage — a
  # different assertion entirely, and not one anyone has made.
  same=$(curl -s --max-time 10 "$EP" --data-urlencode \
    "query=PREFIX chorus: <$NS> SELECT (COUNT(*) AS ?c) WHERE { GRAPH <$PRIN_GRAPH> { ?a chorus:rhymesWith ?b ; chorus:principleKind ?k . ?b chorus:principleKind ?k } }" \
    -H "Accept: application/sparql-results+json" \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['c']['value'])")
  [ "$same" = "0" ]
}
