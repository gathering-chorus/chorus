#!/bin/bash
# #4220 — move Principal rows from the security graph to identity.
#
# Runs against whatever store FUSEKI_BASE names, so the werk store proves it
# before prod ever sees it. Refuses if the counts do not add up: the move is
# copy → verify → delete, never delete-then-hope.
set -u
BASE="${FUSEKI_BASE:-http://localhost:3030/werk-silas}"
N='https://jeffbridwell.com/chorus#'
SRC=urn:chorus:domains:security
DST=urn:chorus:domains:identity
# tr -d '\r' is not cosmetic: Fuseki's CSV ends every line CRLF, and without it
# every [ "$n" -lt ... ] below died with "integer expression expected" — the
# guard printed an error and the script carried on to the DELETE anyway. Caught
# on the first werk rehearsal, 2026-09-19. A guard that cannot compare is a
# guard that is not there (#3734).
q(){ curl -s -G "$BASE/sparql" -H 'Accept: text/csv' --data-urlencode "query=$1" | tail -n +2 | tr -d '\r'; }
AUTH=()
u(){ curl -s -o /dev/null -w '%{http_code}' ${AUTH[@]+"${AUTH[@]}"} -X POST "$BASE/update" --data-urlencode "update=$1"; }
[ -n "${FUSEKI_ADMIN_USER:-}" ] && AUTH=(-u "$FUSEKI_ADMIN_USER:$FUSEKI_ADMIN_PASSWORD")

count(){ q "SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$1> { ?s a <${N}Principal> } }"; }
triples(){ q "SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$1> { ?s a <${N}Principal> ; ?p ?o } }"; }

BEFORE_SRC=$(count $SRC); BEFORE_DST=$(count $DST); BEFORE_T=$(triples $SRC)
echo "before: security=$BEFORE_SRC identity=$BEFORE_DST triples=$BEFORE_T"
[ "${BEFORE_SRC:-0}" -eq 0 ] && { echo "nothing to move"; exit 0; }
# set -e is deliberately NOT used: the refusal below must run, not abort early.

echo "copy  : HTTP $(u "INSERT { GRAPH <$DST> { ?s ?p ?o } } WHERE { GRAPH <$SRC> { ?s a <${N}Principal> ; ?p ?o } }")"
AFTER_DST=$(count $DST); COPIED_T=$(triples $DST)
echo "copied: identity=$AFTER_DST triples=$COPIED_T"
if [ "${AFTER_DST:-0}" -lt "${BEFORE_SRC:-0}" ] || [ "${COPIED_T:-0}" -lt "${BEFORE_T:-0}" ]; then
  echo "REFUSED: copy is short (identity has $AFTER_DST of $BEFORE_SRC rows, $COPIED_T of $BEFORE_T triples) — source left intact"
  exit 1
fi
echo "delete: HTTP $(u "DELETE { GRAPH <$SRC> { ?s ?p ?o } } WHERE { GRAPH <$SRC> { ?s a <${N}Principal> ; ?p ?o } }")"
echo "after : security=$(count $SRC) identity=$(count $DST) triples=$(triples $DST)"
