#!/usr/bin/env bash
# @test-type: unit — reads the CoreShape source through riot; no store
# @domain: domains
# 4590-core-shape-no-exposure.test.sh — #4590. What Jeff sees: every class's
# fields are served, not just its IRI.
#
# The flatten copies CoreShape's fields onto every class shape, and athena-make
# hides every UNMARKED field on a shape that marks any field with chorus:exposure.
# A mark on a CoreShape field therefore switched hiding on for every class that
# had none: on 2026-10-17, 70 classes (principals included) served only their IRI.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
command -v riot >/dev/null || { echo "UNMEASURED: riot not on PATH"; exit 2; }
pass=0; fail=0

marks() { # file -> count of exposure marks on CoreShape's own fields
  riot --output=nt "$1" 2>/dev/null | python3 -c '
import sys
C="<https://jeffbridwell.com/chorus#CoreShape>"; P="<http://www.w3.org/ns/shacl#property>"; E="<https://jeffbridwell.com/chorus#exposure>"
lines=[l.split(" ",2) for l in sys.stdin if l.strip()]
props={o.rstrip(" .\n") for s,p,o in lines if s==C and p==P}
print(sum(1 for s,p,o in lines if s in props and p==E))'
}

n=$(marks "$ROOT/roles/silas/ontology/core-shape-4576.ttl")
if [ "$n" = 0 ]; then echo "PASS CoreShape's fields carry no exposure mark"; pass=$((pass+1)); else echo "FAIL CoreShape carries $n exposure mark(s)"; fail=$((fail+1)); fi

# NEGATIVE PROOF: the shape as it shipped on 10-17, one field marked public, is caught
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
cat > "$T/bad.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
chorus:CoreShape a sh:NodeShape ;
    sh:property [ sh:path chorus:label ; chorus:exposure chorus:public ] ;
    sh:property [ sh:path chorus:comment ] .
TTL
n=$(marks "$T/bad.ttl")
if [ "$n" = 1 ]; then echo "PASS NEGATIVE PROOF: a marked CoreShape field is counted"; pass=$((pass+1)); else echo "FAIL the check cannot see a mark (counted $n)"; fail=$((fail+1)); fi

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
