#!/usr/bin/env bash
# alertfile-optional-4671.sh — #4671: delete AlertShape's old minCount-1 alertFile
# blank node, once, by path. The ttl now names the property (AlertShape-alertFile,
# minCount 0), but the model deploy merges additively, so the old blank node stays
# and the door keeps refusing Grafana-sourced Alert rows.
#
# Scoped by sh:path, never any-predicate (the 10-18 scar: a closure that walked
# blank nodes emptied CoreShape on 105 shapes). COUNT first and refuse unless it
# is exactly 1. No default store: the dataset URL is required, so it is always a
# decision whether this runs on staging or prod (prod needs Jeff's yes).
#
#   alertfile-optional-4671.sh <dataset-url> [--apply]
#     without --apply: counts only
set -euo pipefail

DS="${1:-}"
[ -n "$DS" ] || { echo "usage: $0 <dataset-url, e.g. http://localhost:3030/pods> [--apply]" >&2; exit 2; }
APPLY="${2:-}"
SCRIPTS="$(cd "$(dirname "$0")" && pwd)"

PFX='PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX c: <https://jeffbridwell.com/chorus#>'
WHERE='GRAPH <urn:chorus:ontology> { c:AlertShape sh:property ?p . ?p sh:path c:alertFile . FILTER isBlank(?p) }'

count() {
  curl -sf --max-time 30 -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$PFX SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { $1 }" "$DS/query" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])'
}

# CoreShape's own property count, and empty shape nodes, before and after
CORE='GRAPH <urn:chorus:ontology> { c:CoreShape sh:property ?p }'
EMPTY='GRAPH <urn:chorus:ontology> { ?s sh:property ?p . FILTER NOT EXISTS { ?p ?k ?v } }'

n=$(count "$WHERE"); core=$(count "$CORE"); empty=$(count "$EMPTY")
echo "before: old alertFile blank nodes=$n · CoreShape properties=$core · empty property nodes=$empty"
[ "$n" = "1" ] || { echo "refused: expected exactly 1 old blank node, found $n — nothing deleted"; exit 1; }
[ "$APPLY" = "--apply" ] || { echo "count only (pass --apply to delete)"; exit 0; }

source "$SCRIPTS/fuseki-auth.sh"
curl -sf --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST \
  --data-urlencode "update=$PFX DELETE { GRAPH <urn:chorus:ontology> { c:AlertShape sh:property ?p . ?p ?k ?v } } WHERE { GRAPH <urn:chorus:ontology> { c:AlertShape sh:property ?p . ?p sh:path c:alertFile ; ?k ?v . FILTER isBlank(?p) } }" \
  "$DS/update" >/dev/null

n2=$(count "$WHERE"); core2=$(count "$CORE"); empty2=$(count "$EMPTY")
echo "after:  old alertFile blank nodes=$n2 · CoreShape properties=$core2 · empty property nodes=$empty2"
[ "$n2" = "0" ] && [ "$core2" = "$core" ] && [ "$empty2" = "$empty" ] \
  || { echo "FAILED: the delete did not leave the shape store as expected"; exit 1; }
echo "ok"
