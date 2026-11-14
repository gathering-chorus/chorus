#!/usr/bin/env bats
# @test-type: unit — runs the shipped MonitorShape over a fixture with Jena shacl; no store, no network
# @domain: monitors
# @card: #4970 · owner: silas
# Checks the shape in roles/silas/ontology/alerts-4085.ttl (named in full so werk-test selects this suite).
#
# #4970 AC2 — a monitor names its sensor, its line (exact), one owner and one
# action; one missing any of them is refused. The shape is read from the shipped
# ontology (never copied). The Bedroom row itself is checked in prod by
# 4970-alert-made-from-monitor.bats, which needs the land.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export TMPD="$(mktemp -d)"
  # the fixture lives here, not under platform/tests/fixtures: a non-bats file there
  # selects platform/tests' cucumber package, which jest cannot measure (#4970 run 1)
  cat > "$TMPD/fixture.ttl" <<'TTL'
# #4970 — fixture for MonitorShape: one complete monitor and three that lack the
# sensor, the owner or the action. Names are the expected verdicts' keys.
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:    <http://www.w3.org/2001/XMLSchema#> .
chorus:principal-silas a chorus:Principal .
chorus:monitors a chorus:Domain .
chorus:infra-disk a chorus:Sensor .
chorus:monitor-good a chorus:Monitor ; rdfs:label "good" ; chorus:measures "is a disk nearly full" ; chorus:hasDomain chorus:monitors ;
    chorus:monitorOfSensor chorus:infra-disk ; chorus:monitorLine "99"^^xsd:decimal ; chorus:monitorAction "free space" ; chorus:ownedBy chorus:principal-silas .
chorus:monitor-nosensor a chorus:Monitor ; rdfs:label "nosensor" ; chorus:measures "x" ; chorus:hasDomain chorus:monitors ;
    chorus:monitorLine "99"^^xsd:decimal ; chorus:monitorAction "free space" ; chorus:ownedBy chorus:principal-silas .
chorus:monitor-noowner a chorus:Monitor ; rdfs:label "noowner" ; chorus:measures "x" ; chorus:hasDomain chorus:monitors ;
    chorus:monitorOfSensor chorus:infra-disk ; chorus:monitorLine "99"^^xsd:decimal ; chorus:monitorAction "free space" .
chorus:monitor-noaction a chorus:Monitor ; rdfs:label "noaction" ; chorus:measures "x" ; chorus:hasDomain chorus:monitors ;
    chorus:monitorOfSensor chorus:infra-disk ; chorus:monitorLine "99"^^xsd:decimal ; chorus:ownedBy chorus:principal-silas .
TTL
  export ONT="$ROOT/roles/silas/ontology/alerts-4085.ttl"
  command -v shacl >/dev/null || { echo "Jena shacl not on PATH" >&2; return 1; }
  awk '/^chorus:MonitorShape a sh:NodeShape/,/ownedBy ; sh:minCount 1 ; sh:maxCount 1 ; sh:class chorus:Principal \] \./' "$ONT" \
    | sed 's/ sh:node chorus:CoreShape ;//' > "$TMPD/shape.ttl"
  { printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    printf '@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n'
    cat "$TMPD/shape.ttl"; } > "$TMPD/shapes.ttl"
  shacl validate --shapes "$TMPD/shapes.ttl" --data "$TMPD/fixture.ttl" > "$TMPD/report.ttl" 2>"$TMPD/err" || true
  sparql --data "$TMPD/report.ttl" --results TSV 'PREFIX sh: <http://www.w3.org/ns/shacl#>
    SELECT DISTINCT ?f WHERE { ?r a sh:ValidationResult ; sh:focusNode ?f }' | tail -n +2 | sed 's|.*#monitor-||; s|>||' | sort > "$TMPD/refused"
}
teardown_file() { rm -rf "$TMPD"; }

refused() { grep -qx "$1" "$TMPD/refused"; }

@test "the shipped MonitorShape requires sensor, line, action and owner, and shacl ran clean" {
  for p in monitorOfSensor monitorLine monitorAction ownedBy; do
    grep -q "sh:path chorus:$p ; sh:minCount 1" "$TMPD/shape.ttl"
  done
  [ ! -s "$TMPD/err" ]
}
@test "NEGATIVE PROOF: a monitor with no sensor is refused" { refused nosensor; }
@test "NEGATIVE PROOF: a monitor with no owner is refused" { refused noowner; }
@test "NEGATIVE PROOF: a monitor with no action is refused" { refused noaction; }
@test "a complete monitor conforms" { ! refused good; }
@test "exactly the three built violations, no more" {
  [ "$(tr '\n' ' ' < "$TMPD/refused")" = "noaction noowner nosensor " ]
}
