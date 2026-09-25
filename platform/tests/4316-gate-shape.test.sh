#!/usr/bin/env bash
# @test-type: unit — validates the stage gates from the shipped seed file against the shipped GateShape; no store, no network
#
# #4316 — the seed stopped on "Gate requires 'ownerRole'" (DirectionGate): the
# security GateShape required an owner role on every Gate, and the four stage
# gates carry a gatekeeper instead. The shipped seed rows must conform, and the
# shape must still refuse what it exists to refuse (two owner roles on one gate).
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
# GateShape, as shipped, and nothing else
sparql --data "$ROOT/roles/silas/ontology/security-model-3618.ttl" --results TTL '
  PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX c: <https://jeffbridwell.com/chorus#>
  CONSTRUCT { ?s ?p ?o } WHERE { { c:GateShape ?p ?o . BIND(c:GateShape AS ?s) } UNION { c:GateShape sh:property ?s . ?s ?p ?o } }' > "$TMP/shape.ttl"
grep -q "targetClass" "$TMP/shape.ttl" || { echo "FAIL GateShape not found in security-model-3618.ttl"; exit 1; }
violations() { shacl validate --shapes "$TMP/shape.ttl" --data "$1" | grep "sh:focusNode" | sort -u | grep -c . || true; }  # distinct gates: GateShape declares each rule twice (named + blank-node)

riot --output=ntriples "$ROOT/designing/data/gate-instances.ttl" > "$TMP/gates.nt"
n=$(violations "$TMP/gates.nt")
[ "$n" = "0" ] && { echo "PASS every gate in the seed file conforms (stage gates carry no ownerRole)"; pass=$((pass+1)); } || { echo "FAIL $n gate(s) in the seed file violate GateShape"; fail=$((fail+1)); }

cat > "$TMP/bad.ttl" <<'TTL'
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
c:gate-two-owners a c:Gate ; rdfs:label "two owners" ; c:ownerRole c:role-silas , c:role-wren .
c:gate-no-label a c:Gate .
TTL
n=$(violations "$TMP/bad.ttl")
[ "$n" = "2" ] && { echo "PASS negative proof: the gate with two owner roles and the gate with no label are both refused"; pass=$((pass+1)); } || { echo "FAIL negative proof: got $n violation(s), want 2"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
