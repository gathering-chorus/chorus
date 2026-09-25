#!/usr/bin/env bash
# 4311-owner-move.sh — #4311: documents, skills and gates whose hasOwner names a
# bare #jeff/#wren/#silas/#kade (no such row) get ownedBy chorus:principal-<name>
# and lose the broken hasOwner edge. Rows whose principal is missing are left
# alone. Same shape as #4294's owner move. Backs up first, prints before/after.
#
#   4311-owner-move.sh           # dry run: counts only, writes nothing
#   4311-owner-move.sh --apply   # backup + move + verify
set -euo pipefail

SCRIPTS="$(cd "$(dirname "$0")" && pwd)"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
PFX='PREFIX c: <https://jeffbridwell.com/chorus#>'
# ?s hasOwner ?o in graph ?g, ?o a bare name with no row, ?p its existing principal
MATCH='GRAPH ?g { ?s c:hasOwner ?o }
  FILTER(isIRI(?o) && STRSTARTS(STR(?o), STR(c:)))
  FILTER NOT EXISTS { GRAPH ?g0 { ?o a ?any } }
  BIND(IRI(CONCAT(STR(c:), "principal-", STRAFTER(STR(?o), STR(c:)))) AS ?p)
  FILTER EXISTS { GRAPH ?pg { ?p a c:Principal } }'

count() {
  curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
    "query=$PFX SELECT ?o (COUNT(*) AS ?n) WHERE { $MATCH } GROUP BY ?o"
}

echo "== rows to move =="
count

[ "${1:-}" = "--apply" ] || { echo "(dry run — rerun with --apply to move them)"; exit 0; }

backup="$HOME/.chorus/backups/owner-move-4311-$(date +%Y%m%dT%H%M%S).csv"
mkdir -p "$(dirname "$backup")"
curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
  "query=$PFX SELECT ?g ?s ?o WHERE { $MATCH }" > "$backup"
echo "== backup: $backup ($(($(wc -l < "$backup") - 1)) rows) =="

source "$SCRIPTS/fuseki-auth.sh"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time "${FUSEKI_WRITE_TIMEOUT:-300}" \
  "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPDATE" --data-urlencode \
  "update=$PFX DELETE { GRAPH ?g { ?s c:hasOwner ?o } } INSERT { GRAPH ?g { ?s c:ownedBy ?p } } WHERE { $MATCH }")
echo "== update HTTP $code =="
[ "$code" = 200 ] || [ "$code" = 204 ] || { echo "FAILED — backup at $backup"; exit 1; }

echo "== rows left (should be empty) =="
count
