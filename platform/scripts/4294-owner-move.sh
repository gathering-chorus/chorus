#!/usr/bin/env bash
# 4294-owner-move.sh — #4294 AC3: move every ownedBy that is a plain name
# ("kade") or chorus:role-jeff to the matching Principal IRI (chorus:principal-kade).
# Every owner maps to a principal that already exists; rows whose principal is
# missing are left alone. Backs up first, then prints before/after counts.
#
#   4294-owner-move.sh           # dry run: counts only, writes nothing
#   4294-owner-move.sh --apply   # backup + move + verify
set -euo pipefail

SCRIPTS="$(cd "$(dirname "$0")" && pwd)"
QUERY=http://localhost:3030/pods/query
UPDATE=http://localhost:3030/pods/update
PFX='PREFIX c: <https://jeffbridwell.com/chorus#>'
# ?s ownedBy ?o in graph ?g, ?o a literal or role-jeff, ?p its existing principal
MATCH='GRAPH ?g { ?s c:ownedBy ?o }
  FILTER(isLiteral(?o) || ?o = c:role-jeff)
  BIND(IRI(CONCAT(STR(c:), "principal-", IF(isLiteral(?o), STR(?o), "jeff"))) AS ?p)
  FILTER EXISTS { GRAPH ?pg { ?p a ?t } }'

count() {
  curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
    "query=$PFX SELECT ?o (COUNT(*) AS ?n) WHERE { $MATCH } GROUP BY ?o"
}

echo "== rows to move =="
count

[ "${1:-}" = "--apply" ] || { echo "(dry run — rerun with --apply to move them)"; exit 0; }

backup="$HOME/.chorus/backups/owner-move-4294-$(date +%Y%m%dT%H%M%S).csv"
mkdir -p "$(dirname "$backup")"
curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
  "query=$PFX SELECT ?g ?s ?o WHERE { $MATCH }" > "$backup"
echo "== backup: $backup ($(($(wc -l < "$backup") - 1)) rows) =="

source "$SCRIPTS/fuseki-auth.sh"
code=$(curl -s -o /dev/stderr -w '%{http_code}' --max-time "${FUSEKI_WRITE_TIMEOUT:-300}" \
  "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPDATE" --data-urlencode \
  "update=$PFX DELETE { GRAPH ?g { ?s c:ownedBy ?o } } INSERT { GRAPH ?g { ?s c:ownedBy ?p } } WHERE { $MATCH }")
echo "== update HTTP $code =="
[ "$code" = 200 ] || [ "$code" = 204 ] || { echo "FAILED — nothing verified; backup at $backup"; exit 1; }

echo "== rows left (should be empty) =="
count
