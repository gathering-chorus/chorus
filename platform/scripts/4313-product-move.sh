#!/usr/bin/env bash
# 4313-product-move.sh — #4313: 169 documents whose hasProduct named a row that
# doesn't exist (#gatheringProduct, #akashaProduct). Each document's product was
# decided by reading it, recorded in 4313-product-map.tsv: chorus, gathering,
# both, or none (consulting work). Backs up first, prints before/after.
#
#   4313-product-move.sh           # dry run: counts only, writes nothing
#   4313-product-move.sh --apply   # backup + move + verify
set -euo pipefail

SCRIPTS="$(cd "$(dirname "$0")" && pwd)"
MAP="$SCRIPTS/4313-product-map.tsv"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
PFX='PREFIX c: <https://jeffbridwell.com/chorus#>'
BROKEN='GRAPH ?g { ?s c:hasProduct ?o } VALUES ?o { c:gatheringProduct c:akashaProduct }'

count() {
  curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
    "query=$PFX SELECT ?o (COUNT(*) AS ?n) WHERE { $BROKEN } GROUP BY ?o"
}

# VALUES rows: (<doc> <product>) for each product a doc gets; "none" adds nothing.
values=$(grep -v '^#' "$MAP" | awk -F'\t' '
  $2=="chorus"    {print "(<"$1"> c:chorus)"}
  $2=="gathering" {print "(<"$1"> c:gathering)"}
  $2=="both"      {print "(<"$1"> c:chorus)"; print "(<"$1"> c:gathering)"}')
docs=$(grep -v '^#' "$MAP" | awk -F'\t' '{print "<"$1">"}')

echo "== broken product links =="; count
echo "== map: $(grep -vc '^#' "$MAP") docs · $(grep -v '^#' "$MAP" | cut -f2 | sort | uniq -c | tr '\n' ' ')"
[ "${1:-}" = "--apply" ] || { echo "(dry run — rerun with --apply to move them)"; exit 0; }

backup="$HOME/.chorus/backups/product-move-4313-$(date +%Y%m%dT%H%M%S).csv"
mkdir -p "$(dirname "$backup")"
curl -s --max-time 60 "$QUERY" -H 'Accept: text/csv' --data-urlencode \
  "query=$PFX SELECT ?g ?s ?o WHERE { $BROKEN }" > "$backup"
echo "== backup: $backup ($(($(wc -l < "$backup") - 1)) rows) =="

source "$SCRIPTS/fuseki-auth.sh"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time "${FUSEKI_WRITE_TIMEOUT:-300}" \
  "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPDATE" --data-urlencode \
  "update=$PFX
DELETE { GRAPH ?g { ?s c:hasProduct ?o } } WHERE { VALUES ?s { $docs } $BROKEN } ;
INSERT { GRAPH <urn:chorus:documents> { ?s c:hasProduct ?p } } WHERE { VALUES (?s ?p) { $values } GRAPH <urn:chorus:documents> { ?s a ?t } }")
echo "== update HTTP $code =="
[ "$code" = 200 ] || [ "$code" = 204 ] || { echo "FAILED — backup at $backup"; exit 1; }

echo "== broken links left (should be empty) =="; count
