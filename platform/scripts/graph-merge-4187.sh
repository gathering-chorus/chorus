#!/usr/bin/env bash
# #4187 — fold one subject into another. Jeff, 2026-09-18: "feels like a merge
# problem" — and it is. borg/borgProduct and chorus/chorusProduct are one thing
# each, written twice: the substance sits on the short name (92 and 57 triples),
# the references sit on the *Product name (39 and 135 inbound). Neither is a
# subset of the other, so choosing a winner loses data either way.
#
# A merge does three things, in this order, and verifies each:
#   1. copy FROM's properties onto TO, in whichever graph they live
#   2. rewrite every edge pointing AT From to point at TO
#   3. delete FROM's own triples
#
# Read-only by default. Refuses if step 1 or 2 leaves anything behind.
set -euo pipefail
FROM="${1:?usage: graph-merge-4187.sh <from-local> <to-local> [--go]}"
TO="${2:?usage: graph-merge-4187.sh <from-local> <to-local> [--go]}"
MODE="${3:-}"
NS="https://jeffbridwell.com/chorus#"
QRY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPD="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
FU="${FUSEKI_USER:-${FUSEKI_ADMIN_USER:-}}"; FP="${FUSEKI_PASSWORD:-${FUSEKI_ADMIN_PASSWORD:-}}"
AUTH=(); [ -n "$FU" ] && AUTH=(-u "$FU:$FP")
[ -n "$FU" ] || echo "note: no store credentials — source platform/scripts/fuseki-auth.sh first"
ask() { curl -sf ${AUTH[@]+"${AUTH[@]}"} --max-time 60 -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=PREFIX c: <$NS> SELECT (COUNT(*) AS ?n) WHERE { $1 }" "$QRY" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])'; }
own()  { ask "GRAPH ?g { c:$1 ?p ?o }"; }
inb()  { ask "GRAPH ?g { ?s ?p c:$1 }"; }
upd() { local code
  code=$(curl -s ${AUTH[@]+"${AUTH[@]}"} --max-time 300 -o /dev/null -w '%{http_code}' -X POST "$UPD" --data-urlencode "update=$1")
  [ "$code" = "200" ] || [ "$code" = "204" ] || { echo "REFUSED: store answered $code"; exit 1; }; }
echo "$FROM  own $(own "$FROM")  inbound $(inb "$FROM")"
echo "$TO    own $(own "$TO")    inbound $(inb "$TO")"
[ "$MODE" = "--go" ] || { echo "(read-only — pass --go to merge $FROM into $TO)"; exit 0; }
# 1. properties across, in FROM's own graph so the row stays where it lives
upd "PREFIX c: <$NS> INSERT { GRAPH ?g { c:$TO ?p ?o } } WHERE { GRAPH ?g { c:$FROM ?p ?o } FILTER(?p != <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> || true) }"
# 2. inbound edges repointed
upd "PREFIX c: <$NS> DELETE { GRAPH ?g { ?s ?p c:$FROM } } INSERT { GRAPH ?g { ?s ?p c:$TO } } WHERE { GRAPH ?g { ?s ?p c:$FROM } }"
left=$(inb "$FROM"); [ "$left" = "0" ] || { echo "REFUSED: $left inbound edge(s) still point at $FROM"; exit 1; }
# 3. the old subject goes
upd "PREFIX c: <$NS> DELETE { GRAPH ?g { c:$FROM ?p ?o } } WHERE { GRAPH ?g { c:$FROM ?p ?o } }"
echo "after: $FROM own $(own "$FROM") inbound $(inb "$FROM")  |  $TO own $(own "$TO") inbound $(inb "$TO")"
