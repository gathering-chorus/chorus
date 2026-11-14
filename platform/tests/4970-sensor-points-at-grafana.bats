#!/usr/bin/env bats
# @test-type: unit — runs the shipped SensorShape rule over a fixture with Jena shacl; no store, no network
# @domain: heralds
# @card: #4970 · owner: silas
# Checks the rule in roles/silas/ontology/sensors-4405.ttl (named in full so werk-test selects this suite).
#
# #4970 AC1 — a sensor row can point at Grafana: source grafana, a datasource and a
# query. A grafana sensor with no query is refused. The rule is read from the
# shipped ontology (never copied), so a renamed or deleted rule fails here.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export TMPD="$(mktemp -d)"
  command -v shacl >/dev/null || { echo "Jena shacl not on PATH" >&2; return 1; }
  export ONT="$ROOT/roles/silas/ontology/sensors-4405.ttl"
  awk '/^chorus:SensorPointsSomewhere a sh:SPARQLConstraint/,/""" \./' "$ONT" > "$TMPD/rule.ttl"
  { printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    printf 'chorus:SensorPointsTest a sh:NodeShape ; sh:targetClass chorus:Sensor ; sh:sparql chorus:SensorPointsSomewhere .\n'
    cat "$TMPD/rule.ttl"; } > "$TMPD/shapes.ttl"
  shacl validate --shapes "$TMPD/shapes.ttl" --data "$ROOT/platform/tests/fixtures/sensor-points-4970.ttl" > "$TMPD/report.ttl" 2>"$TMPD/err" || true
  sparql --data "$TMPD/report.ttl" --results TSV 'PREFIX sh: <http://www.w3.org/ns/shacl#>
    SELECT DISTINCT ?f WHERE { ?r a sh:ValidationResult ; sh:focusNode ?f }' | tail -n +2 | sed 's|.*#sensor-||; s|>||' | sort > "$TMPD/refused"
}
teardown_file() { rm -rf "$TMPD"; }

refused() { grep -qx "$1" "$TMPD/refused"; }

@test "the rule is on the shipped SensorShape and shacl ran clean" {
  [ -s "$TMPD/rule.ttl" ]
  grep -q 'sh:sparql chorus:SensorPointsSomewhere' "$ONT"
  [ ! -s "$TMPD/err" ]
}
@test "the Sensor shape takes a source, a datasource and a query" {
  for p in sensorSource sensorDatasource sensorQuery; do
    grep -q "sh:path chorus:$p ;" "$ONT"
  done
}
@test "NEGATIVE PROOF: a grafana sensor with no query is refused" { refused noquery; }
@test "NEGATIVE PROOF: a grafana sensor with no datasource or an empty query is refused" { refused nods; refused emptyq; }
@test "NEGATIVE PROOF: a sensor that is neither a pass nor a pointer is refused" { refused nothing; }
@test "a crawl pass and a complete grafana pointer conform" { ! refused files; ! refused grafana; }
@test "exactly the four built violations, no more" {
  [ "$(tr '\n' ' ' < "$TMPD/refused")" = "emptyq nods noquery nothing " ]
}
