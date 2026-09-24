#!/usr/bin/env bash
# #4187 — empty the retired catch-all graph urn:chorus:instances, by hand.
#
# What is in it (measured 2026-09-24 09:13): Endpoint 448 and Page 15 written by
# the retired discover-* routes (the crawler's rows live in urn:chorus:domains:code,
# 263 + 21, different IRIs), and one PriorArt row that belongs to the roles domain.
#
#   4187-empty-catch-all.sh          # counts only, writes nothing
#   4187-empty-catch-all.sh --go     # re-home the PriorArt row, delete the rest, count again
#
# Every write goes through Fuseki with the admin credentials from fuseki-auth.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/platform/scripts/fuseki-auth.sh"
Q="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
U="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
NS="https://jeffbridwell.com/chorus#"
PRIOR="${NS}roles-domain-prior-art-org-design-artifact"

count() { # per-class typed rows in the catch-all
  curl -s --max-time 60 "${FUSEKI_AUTH[@]}" -G "$Q" -H 'Accept: text/csv' \
    --data-urlencode 'query=SELECT ?t (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?s a ?t } } GROUP BY ?t ORDER BY DESC(?n)' \
    | tail -n +2 | tr -d '\r' | sed "s|$NS||" | awk -F, '{printf "  %-16s %s\n", $1, $2} END {if (NR==0) print "  (empty)"}'
}
triples() {
  curl -s --max-time 60 "${FUSEKI_AUTH[@]}" -G "$Q" -H 'Accept: text/csv' \
    --data-urlencode 'query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?s ?p ?o } }' | tail -1 | tr -d '\r'
}

echo "urn:chorus:instances before ($(triples) triples):"; count

if [ "${1:-}" != "--go" ]; then
  echo
  echo "dry run. With --go: copy <${PRIOR}> into urn:chorus:domains:roles, then DELETE every triple in urn:chorus:instances."
  exit 0
fi

UPDATE="INSERT { GRAPH <urn:chorus:domains:roles> { <$PRIOR> ?p ?o } } WHERE { GRAPH <urn:chorus:instances> { <$PRIOR> ?p ?o } } ;
DELETE WHERE { GRAPH <urn:chorus:instances> { ?s ?p ?o } }"
code="$(curl -s --max-time 300 "${FUSEKI_AUTH[@]}" -o /dev/null -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' --data-binary "$UPDATE" "$U")"
case "$code" in 2*) ;; *) echo "update refused: HTTP $code" >&2; exit 1 ;; esac

echo
echo "urn:chorus:instances after ($(triples) triples):"; count
echo "PriorArt row in urn:chorus:domains:roles: $(curl -s --max-time 30 "${FUSEKI_AUTH[@]}" -G "$Q" -H 'Accept: text/csv' \
  --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:domains:roles> { <$PRIOR> ?p ?o } }" | tail -1 | tr -d '\r') triples"
