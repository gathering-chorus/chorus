#!/usr/bin/env bash
# 4314-label-twin.sh — #4314: the 528 rows athena-validate flagged for a missing
# label (list: 4314-label-rows.txt, from the 2026-09-25 17:10 run). Almost all
# carry one of the two label names the model's twin rule (ADR-028, #4291) wants
# on every row; this copies the one they have into the one they lack, in the
# same graph. Nothing else changes. Backs up first.
#
#   4314-label-twin.sh           # dry run: counts only
#   4314-label-twin.sh --apply   # backup + copy + verify
set -euo pipefail
SCRIPTS="$(cd "$(dirname "$0")" && pwd)"
QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
P='PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>'
# row name = the IRI's local part (the door's own naming for these kinds)
iris=$(grep -v '^\s*$' "$SCRIPTS/4314-label-rows.txt" | sed 's|.*|<https://jeffbridwell.com/chorus#&>|' | tr '\n' ' ')
ROWS="VALUES ?s { $iris } GRAPH ?g { ?s a ?t } FILTER(STRSTARTS(STR(?g),\"urn:chorus:domains:\"))"

count() {
  curl -s --max-time 280 "$QUERY" -H 'Accept: text/csv' --data-urlencode "query=$P SELECT
    (SUM(IF(EXISTS{GRAPH ?g {?s c:label ?a}} && !EXISTS{GRAPH ?g {?s rdfs:label ?b}},1,0)) AS ?needRdfs)
    (SUM(IF(EXISTS{GRAPH ?g {?s rdfs:label ?a}} && !EXISTS{GRAPH ?g {?s c:label ?b}},1,0)) AS ?needChorus)
    (SUM(IF(!EXISTS{GRAPH ?g {?s rdfs:label ?a}} && !EXISTS{GRAPH ?g {?s c:label ?b}},1,0)) AS ?neither)
    WHERE { SELECT DISTINCT ?s ?g WHERE { $ROWS } }"
}
echo "== before =="; count
[ "${1:-}" = "--apply" ] || { echo "(dry run)"; exit 0; }
backup="$HOME/.chorus/backups/label-twin-4314-$(date +%Y%m%dT%H%M%S).csv"
curl -s --max-time 280 "$QUERY" -H 'Accept: text/csv' --data-urlencode "query=$P SELECT DISTINCT ?g ?s ?cl ?rl WHERE { $ROWS OPTIONAL { GRAPH ?g { ?s c:label ?cl } } OPTIONAL { GRAPH ?g { ?s rdfs:label ?rl } } }" > "$backup"
echo "== backup: $backup ($(($(wc -l < "$backup") - 1)) lines) =="
source "$SCRIPTS/fuseki-auth.sh"
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 300 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPDATE" --data-urlencode "update=$P
INSERT { GRAPH ?g { ?s rdfs:label ?l } } WHERE { $ROWS GRAPH ?g { ?s c:label ?l } FILTER NOT EXISTS { GRAPH ?g { ?s rdfs:label ?x } } } ;
INSERT { GRAPH ?g { ?s c:label ?l } } WHERE { $ROWS GRAPH ?g { ?s rdfs:label ?l } FILTER NOT EXISTS { GRAPH ?g { ?s c:label ?x } } }")
echo "== update HTTP $code =="; [ "$code" = 200 ] || [ "$code" = 204 ] || exit 1
echo "== after (needRdfs and needChorus should be 0) =="; count
