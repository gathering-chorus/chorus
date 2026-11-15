#!/usr/bin/env bats
# @test-type: unit — runs the shipped MonitorShape over the five monitor rows (as Turtle) with Jena shacl; no store
# @domain: monitors
# @card: #4977 · owner: silas
# Checks designing/domains/monitors/*-monitor-4977.json against roles/silas/ontology/alerts-4085.ttl.
#
# #4977 AC2 — five monitors exist as rows (crawler out of sync, sensor run not green,
# SHACL above baseline, Buzz shadow, Fuseki memory; the crawler also has its
# gone-quiet monitor), each with its sensor, a numeric line, one owner and one
# action, the line a number in the row.

setup_file() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export ROOT TMPD="$(mktemp -d)"
  command -v shacl >/dev/null || { echo "Jena shacl not on PATH" >&2; return 1; }
  awk '/^chorus:MonitorShape a sh:NodeShape/,/ownedBy ; sh:minCount 1 ; sh:maxCount 1 ; sh:class chorus:Principal \] \./' "$ROOT/roles/silas/ontology/alerts-4085.ttl" \
    | sed 's/ sh:node chorus:CoreShape ;//' > "$TMPD/shape.ttl"
  { printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix sh: <http://www.w3.org/ns/shacl#> .\n'
    printf '@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n'
    cat "$TMPD/shape.ttl"; } > "$TMPD/shapes.ttl"
}
teardown_file() { rm -rf "$TMPD"; }

FIVE="crawler-out-of-sync crawler-sync-quiet sensor-run-not-green shacl-above-baseline buzz-shadow fuseki-memory"

# monitor JSON (door shape) -> Turtle, as the door would store it; $2 drops one field
ttl() {
  python3 - "$ROOT/designing/domains/monitors/$1-monitor-4977.json" "${2:-}" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); drop = sys.argv[2]
q = lambda s: '"' + str(s).replace('\\', '\\\\').replace('"', '\\"') + '"'
n = m["id"].split(":")[-1]; d = m["data"]; l = m["links"]
t = [f"chorus:{n} a chorus:Monitor", f"rdfs:label {q(d['label'])}", f"chorus:measures {q(d['measures'])}"]
if drop != "monitorLine": t.append(f'chorus:monitorLine "{d["monitorLine"]}"^^xsd:decimal')
if drop != "monitorAction": t.append(f"chorus:monitorAction {q(d['monitorAction'])}")
for k in ("monitorWhen", "monitorSays", "monitorFor", "monitorKind"):
    if k in d: t.append(f"chorus:{k} {q(d[k])}")
for k in ("monitorOfSensor", "ownedBy", "hasDomain"):
    if drop != k: t.append(f"chorus:{k} {l[k]}")
print(" ;\n  ".join(t) + " .")
print(f"{l['monitorOfSensor']} a chorus:Sensor . {l['ownedBy']} a chorus:Principal . {l['hasDomain']} a chorus:Domain .")
PY
}
refused() {
  { printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n'; cat "$1"; } > "$1.ttl"
  shacl validate --shapes "$TMPD/shapes.ttl" --data "$1.ttl" 2>/dev/null | grep -q 'sh:ValidationResult'
}

@test "the five (and the crawler's quiet monitor) conform to the shipped MonitorShape" {
  for m in $FIVE; do ttl "$m" > "$TMPD/$m"; ! refused "$TMPD/$m" || { echo "$m refused"; return 1; }; done
}
@test "each line is a plain number in the row, and each names one owner and one action" {
  for m in $FIVE; do
    python3 -c "import json,re,sys;d=json.load(open(sys.argv[1]));assert re.fullmatch(r'-?[0-9]+(\.[0-9]+)?',d['data']['monitorLine']);assert d['data']['monitorAction'];assert d['links']['ownedBy'].startswith('chorus:principal-')" "$ROOT/designing/domains/monitors/$m-monitor-4977.json" || { echo "$m"; return 1; }
  done
}
@test "NEGATIVE PROOF: any of the five without its action, its sensor or its owner is refused" {
  for drop in monitorAction monitorOfSensor ownedBy; do
    ttl fuseki-memory "$drop" > "$TMPD/f-$drop"
    refused "$TMPD/f-$drop" || { echo "not refused without $drop"; return 1; }
  done
}
