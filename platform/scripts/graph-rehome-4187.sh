#!/usr/bin/env bash
# #4187 — move one class's rows from the v1 catch-all into its domain graph.
#
# WHY THIS EXISTS AND WHY IT IS NOT THE OBVIOUS THING
# athena-make already derives a class's graph: resolve_instances_graph(declared,
# domain) returns the shape's chorus:instancesGraph if declared, else
# urn:chorus:domains:<domain> from the domain that definesVocabulary the class.
# ONE function feeds both the write and every serve read (lib.rs:899, :1201).
#
# So deleting a stale pin moves reads and writes together — which is exactly why
# the rows must move FIRST. Delete the pin while the rows are still in the
# catch-all and the route reads an empty graph: /cards serves 0. That is the
# /domains=0 defect, and it is what #3686 added the pins to avoid.
#
# ORDER, and it is not negotiable:
#   1. this script — copy rows into the domain graph (additive, reversible)
#   2. verify the copy count matches
#   3. delete the chorus:instancesGraph line from the shape, deploy the model
#   4. verify the serve route still answers the same count
#   5. --prune to delete the v1 copies
#
# Steps 1 and 5 write to the live store, so both require a human hand.
set -euo pipefail
CLASS="${1:?usage: graph-rehome-4187.sh <Class> <domain-slug> [--go|--prune]}"
DOMAIN="${2:?usage: graph-rehome-4187.sh <Class> <domain-slug> [--go|--prune]}"
MODE="${3:-}"
NS="https://jeffbridwell.com/chorus#"
SRC="urn:chorus:instances"; DST="urn:chorus:domains:${DOMAIN}"
QRY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
UPD="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
# fuseki-auth.sh exports FUSEKI_ADMIN_USER/_PASSWORD; accept either spelling.
# Getting this wrong made curl -sf 401 and set -e abort with no message at all,
# which is how a refused write looked identical to a no-op (2026-09-18 11:35).
FU="${FUSEKI_USER:-${FUSEKI_ADMIN_USER:-}}"; FP="${FUSEKI_PASSWORD:-${FUSEKI_ADMIN_PASSWORD:-}}"
AUTH=(); [ -n "$FU" ] && AUTH=(-u "$FU:$FP")
[ -n "$FU" ] || echo "note: no store credentials in the environment — source platform/scripts/fuseki-auth.sh first"
count() { curl -sf ${AUTH[@]+"${AUTH[@]}"} --max-time 60 -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=PREFIX c: <$NS> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$1> { ?s a c:$CLASS } }" "$QRY" \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["results"]["bindings"][0]["n"]["value"])'; }
echo "$CLASS: $SRC=$(count "$SRC")  $DST=$(count "$DST")"
case "$MODE" in
  --go)
    code=$(curl -s ${AUTH[@]+"${AUTH[@]}"} --max-time 300 -o /dev/null -w '%{http_code}' -X POST "$UPD" --data-urlencode \
      "update=PREFIX c: <$NS> INSERT { GRAPH <$DST> { ?s ?p ?o } } WHERE { GRAPH <$SRC> { ?s a c:$CLASS ; ?p ?o } }")
    [ "$code" = "200" ] || [ "$code" = "204" ] || { echo "REFUSED: the store answered $code on the copy (401 means no credentials)"; exit 1; }
    a=$(count "$SRC"); b=$(count "$DST")
    echo "after copy: $SRC=$a  $DST=$b"
    [ "$a" = "$b" ] || { echo "REFUSED: copy is short — $a in source, $b in destination"; exit 1; }
    echo "copy verified. Next: delete the shape's chorus:instancesGraph line, deploy, check the route, then --prune." ;;
  --prune)
    b=$(count "$DST"); [ "$b" != "0" ] || { echo "REFUSED: destination is empty, nothing was copied"; exit 1; }
    code=$(curl -s ${AUTH[@]+"${AUTH[@]}"} --max-time 300 -o /dev/null -w '%{http_code}' -X POST "$UPD" --data-urlencode \
      "update=PREFIX c: <$NS> DELETE { GRAPH <$SRC> { ?s ?p ?o } } WHERE { GRAPH <$SRC> { ?s a c:$CLASS ; ?p ?o } }")
    [ "$code" = "200" ] || [ "$code" = "204" ] || { echo "REFUSED: the store answered $code on the prune"; exit 1; }
    echo "after prune: $SRC=$(count "$SRC")  $DST=$(count "$DST")" ;;
  *) echo "(read-only — pass --go to copy, --prune to remove the v1 copies)" ;;
esac
