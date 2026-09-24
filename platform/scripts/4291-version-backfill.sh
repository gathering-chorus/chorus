#!/usr/bin/env bash
# #4291 — bring the existing Version rows up to their own shape. Additive only:
# nothing is deleted or moved.
#
#   rdfs:label   copied from the row's chorus:label (the model's "writable twin",
#                hats-4175.ttl; ADR-028: every record declares rdfs:label).
#                The DAL writes both names from #4291 on; this covers the rows
#                written before it (18,232 on 2026-09-24).
#   writeCount   the vN at the end of the row's own IRI. That is exactly how the
#                writer has stamped it since 2026-09-20: measured 2,046 of 2,046
#                stamped rows equal their IRI's vN, and all 16,186 unstamped rows
#                carry a -vN suffix.
#
#   4291-version-backfill.sh          # counts only, writes nothing
#   4291-version-backfill.sh --go     # write both, count again
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/platform/scripts/fuseki-auth.sh"
Q="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
U="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
G="urn:chorus:domains:provenance"
P='PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> '

n() { curl -s --max-time 120 "${FUSEKI_AUTH[@]}" -G "$Q" -H 'Accept: text/csv' --data-urlencode "query=$P $1" | tail -1 | tr -d '\r'; }
report() {
  echo "  Version rows            $(n "SELECT (COUNT(?v) AS ?n) WHERE { GRAPH <$G> { ?v a c:Version } }")"
  echo "  without rdfs:label      $(n "SELECT (COUNT(?v) AS ?n) WHERE { GRAPH <$G> { ?v a c:Version FILTER NOT EXISTS { ?v rdfs:label ?l } } }")"
  echo "  without writeCount      $(n "SELECT (COUNT(?v) AS ?n) WHERE { GRAPH <$G> { ?v a c:Version FILTER NOT EXISTS { ?v c:writeCount ?w } } }")"
  echo "  writeCount != IRI vN    $(n "SELECT (COUNT(?v) AS ?n) WHERE { GRAPH <$G> { ?v a c:Version ; c:writeCount ?w } FILTER(REPLACE(STR(?v), '^.*-v([0-9]+)\$', '\$1') != STR(?w)) }")"
}

echo "before:"; report
if [ "${1:-}" != "--go" ]; then
  echo; echo "dry run. With --go: add rdfs:label from chorus:label, and writeCount from the IRI's vN, to every Version row missing one."
  exit 0
fi

UPDATE="$P
INSERT { GRAPH <$G> { ?v rdfs:label ?l } } WHERE { GRAPH <$G> { ?v a c:Version ; c:label ?l FILTER NOT EXISTS { ?v rdfs:label ?x } } } ;
INSERT { GRAPH <$G> { ?v c:writeCount ?wc } } WHERE { GRAPH <$G> { ?v a c:Version FILTER NOT EXISTS { ?v c:writeCount ?x } FILTER(REGEX(STR(?v), '-v[0-9]+\$')) BIND(REPLACE(STR(?v), '^.*-v([0-9]+)\$', '\$1') AS ?wc) } }"
code="$(curl -s --max-time 600 "${FUSEKI_AUTH[@]}" -o /dev/null -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' --data-binary "$UPDATE" "$U")"
case "$code" in 2*) ;; *) echo "update refused: HTTP $code" >&2; exit 1 ;; esac
echo; echo "after:"; report
