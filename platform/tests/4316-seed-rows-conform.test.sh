#!/usr/bin/env bash
# @test-type: unit — reads two shipped seed files; no store, no network
#
# #4316 — after the Gate fix, the seed would stop next on two rows of this kind:
# 14 card stubs with chorus:label but no rdfs:label (CardShape requires one), and
# pk-responseWordCap with lifecycleEnabled "false" (a string; the shape wants
# xsd:boolean). Every card stub must carry rdfs:label, and every lifecycleEnabled
# must be a boolean. The negative fixture has one of each defect.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
bad_rows() {
  sparql --data "$1" --results CSV '
    PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
    SELECT DISTINCT ?s WHERE {
      { ?s a c:Card FILTER NOT EXISTS { ?s rdfs:label ?l } }
      UNION { ?s c:lifecycleEnabled ?v FILTER(datatype(?v) != xsd:boolean) } }' | tail -n +2 | grep -c . || true
}
for f in roles/kade/ontology/commitment-card-stubs.ttl designing/data/property-key-instances.ttl; do
  n=$(bad_rows "$ROOT/$f")
  [ "$n" = "0" ] && { echo "PASS $f: every row conforms"; pass=$((pass+1)); } || { echo "FAIL $f: $n row(s) would stop the seed"; fail=$((fail+1)); }
done
cat > "$TMP/bad.ttl" <<'TTL'
@prefix c: <https://jeffbridwell.com/chorus#> .
c:card-9999 a c:Card ; c:label "9999" .
c:pk-bad a c:PropertyKey ; c:lifecycleEnabled "false" .
TTL
n=$(bad_rows "$TMP/bad.ttl")
[ "$n" = "2" ] && { echo "PASS negative proof: the stub with no rdfs:label and the string boolean are both caught"; pass=$((pass+1)); } || { echo "FAIL negative proof: got $n, want 2"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
