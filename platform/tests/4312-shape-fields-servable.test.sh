#!/usr/bin/env bash
# @test-type: unit — applies athena-make's own field-name rule (ADR-040 L4) to the shipped session shapes; no store, no network
#
# #4312 — #4302 landed SessionRun and Presence, and athena-make refused to serve
# both: "adr040-violation: property 'http://purl.org/dc/terms/created' is not
# camelCase". The generator names a field by what follows the last '#' of the
# path (athena-make lib.rs: REPLACE(STR(?path), '.*#', '')), so a path with no
# '#' (dcterms) keeps its whole IRI and fails the camelCase check. The local
# SHACL test passed because SHACL has no such rule: the generator is a second
# reader. This runs the generator's rule on the shipped file before a land.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TTL="${SHAPES_TTL:-$ROOT/roles/silas/ontology/session-model-4302.ttl}"
pass=0; fail=0

rows=$(sparql --data "$TTL" --results TSV '
  PREFIX sh: <http://www.w3.org/ns/shacl#>
  PREFIX c: <https://jeffbridwell.com/chorus#>
  SELECT ?shape ?field WHERE {
    VALUES ?shape { c:SessionRunShape c:PresenceShape }
    ?shape sh:property ?p . ?p sh:path ?path . FILTER(isIRI(?path))
    BIND(REPLACE(STR(?path), ".*#", "") AS ?field)
  }' | tail -n +2 | tr -d '"' | sed 's|<https://jeffbridwell.com/chorus#||; s|>||')
[ -n "$rows" ] || { echo "FAIL: no property shapes read for SessionRunShape / PresenceShape (a renamed shape must fail, not pass)"; exit 1; }

while IFS=$'\t' read -r shape field; do
  if printf '%s' "$field" | grep -qE '^[a-z][A-Za-z0-9]*$'; then pass=$((pass+1))
  else echo "FAIL $shape: field '$field' is not camelCase — athena-make will refuse to serve the class"; fail=$((fail+1)); fi
done <<< "$rows"

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
