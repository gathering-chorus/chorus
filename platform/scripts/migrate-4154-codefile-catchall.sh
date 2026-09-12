#!/usr/bin/env bash
# migrate-4154-codefile-catchall.sh — #4154, one shot, run at land.
# Deletes the chorus:CodeFile rows that POST /api/athena/discover-code (retired by
# #4154) wrote into urn:chorus:instances, a graph ADR-051 froze. Nothing reads them;
# the one walker re-crawls code files into urn:chorus:domains:code through /codefiles.
# Prints the count before and after; refuses to run against a store it cannot count.
set -u
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
[ "${CONFIRM:-}" = "yes" ] || { echo "migrate-4154: dry run. CONFIRM=yes to delete. Store: $FUSEKI_UPDATE"; }
count() {
  curl -sf -H 'Accept: text/csv' --data-urlencode 'query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?s) AS ?n) WHERE { GRAPH <urn:chorus:instances> { ?s a c:CodeFile } }' "$FUSEKI_QUERY" | tail -1 | tr -dc '0-9'
}
before=$(count); [ -n "$before" ] || { echo "migrate-4154: could not count — refusing" >&2; exit 2; }
echo "migrate-4154: CodeFile rows in urn:chorus:instances before: $before"
[ "${CONFIRM:-}" = "yes" ] || exit 0
[ -n "${FUSEKI_AUTH:-}" ] || { echo "migrate-4154: FUSEKI_AUTH not set (source platform/scripts/fuseki-auth.sh)" >&2; exit 2; }
code=$(curl -s -o /dev/null -w '%{http_code}' -u "$FUSEKI_AUTH" -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary 'PREFIX c: <https://jeffbridwell.com/chorus#> DELETE { GRAPH <urn:chorus:instances> { ?s ?p ?o } } WHERE { GRAPH <urn:chorus:instances> { ?s a c:CodeFile ; ?p ?o } }' "$FUSEKI_UPDATE")
after=$(count)
echo "migrate-4154: delete http $code; after: $after"
[ "$after" = "0" ]
