#!/usr/bin/env bats
# @test-type: contract — runs SHACL over fixtures against the repo's AlertShape (roles/silas/ontology/alerts-4085.ttl)
# @domain: alerts — the domain whose rows the Grafana framework lands (#4091)
# plus the fields roles/kade/ontology/alerts-grafana-4091.ttl adds; no store, no network.
# #4091 AC1/AC3/AC6 — an Alert row can carry what a Grafana rule needs, the 46
# prod rows (none of those fields) still conform, and a row with no domain edge
# is refused at the shape.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  SHAPES="$BATS_TEST_TMPDIR/shapes.ttl"
  # AlertShape whole (its blank property nodes come along in Turtle form), minus
  # sh:node CoreShape, plus this card's named property shapes
  riot --output=ttl "$REPO/roles/silas/ontology/alerts-4085.ttl" "$REPO/roles/kade/ontology/alerts-grafana-4091.ttl" 2>/dev/null \
    | grep -v 'sh:node *chorus:CoreShape' > "$SHAPES"
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:infrastructure a chorus:Domain .
'
  BASE='rdfs:label "Host memory pressure" ; chorus:alertSource "grafana" ; chorus:alertFile "chorus/proving/domains/alerts/host-pressure.yml"'
}

verdict() {
  local out
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>&1)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"
  elif echo "$out" | grep -q "sh:conforms *false"; then echo "VIOLATES"
  else echo "ERROR"; fi
}
fixture() { printf '%s%s\n' "$PREFIX" "$1" > "$BATS_TEST_TMPDIR/$2.ttl"; echo "$BATS_TEST_TMPDIR/$2.ttl"; }

@test "control: a prod-shaped Alert row with none of the new fields conforms" {
  f=$(fixture "chorus:a1 a chorus:Alert ; $BASE ; chorus:hasDomain chorus:infrastructure ." a1)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "control: an Alert row carrying everything a Grafana rule needs conforms" {
  f=$(fixture "chorus:a2 a chorus:Alert ; $BASE ; chorus:hasDomain chorus:infrastructure ; chorus:metricQuery \"chorus_host_pressure >= 4\" ; chorus:metricDatasource \"prometheus\" ; chorus:forDuration \"5m\" ; chorus:evalInterval \"1m\" ; chorus:grafanaRuleUid \"host-pressure\" ." a2)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

# NEGATIVE PROOF (AC6): a row with no domain edge is refused at the shape.
@test "an Alert row with no domain is refused" {
  f=$(fixture "chorus:a3 a chorus:Alert ; $BASE ." a3)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a forDuration that is not a Grafana duration is refused" {
  f=$(fixture "chorus:a4 a chorus:Alert ; $BASE ; chorus:hasDomain chorus:infrastructure ; chorus:forDuration \"five minutes\" ." a4)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "a grafanaRuleUid Grafana would reject is refused" {
  f=$(fixture "chorus:a5 a chorus:Alert ; $BASE ; chorus:hasDomain chorus:infrastructure ; chorus:grafanaRuleUid \"has spaces/and a slash\" ." a5)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "two queries on one row are refused" {
  f=$(fixture "chorus:a6 a chorus:Alert ; $BASE ; chorus:hasDomain chorus:infrastructure ; chorus:metricQuery \"a\" , \"b\" ." a6)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}
