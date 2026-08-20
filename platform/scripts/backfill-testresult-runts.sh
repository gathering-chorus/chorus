#!/usr/bin/env bash
# #3925 — backfill runTs onto existing chorus:TestResult rows by PARSING the
# minted name (testresult-<card>-<tsmillis>-<idx>) each row already carries.
# Deterministic: one clock, already recorded — no guessing. Idempotent: only
# rows MISSING runTs are touched. Dry-run by default; --apply writes.
#
# After 137,184/137,184 carry runTs, the shape tightens to minCount 1 (bump 2).
set -euo pipefail
FUSEKI="${FUSEKI_URL:-http://localhost:3030/pods}"
APPLY="${1:-}"
source "$(dirname "$0")/fuseki-auth.sh" 2>/dev/null || true

COUNT_MISSING='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT (COUNT(?r) AS ?n) WHERE { GRAPH <urn:chorus:domains:tests> {
  ?r a chorus:TestResult . FILTER NOT EXISTS { ?r chorus:runTs ?t } } }'
missing=$(curl -sf --max-time 30 -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=$COUNT_MISSING" "$FUSEKI/query" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])")
TOTAL_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT (COUNT(?r) AS ?n) WHERE { GRAPH <urn:chorus:domains:tests> { ?r a chorus:TestResult } }'
total=$(curl -sf --max-time 30 -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=$TOTAL_Q" "$FUSEKI/query" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])")
# a zero TOTAL means the query is aimed at nothing — refuse to report success
[ "$total" -gt 0 ] || { echo "FAIL: query sees 0 TestResults — wrong class/graph, refusing (hollow check)"; exit 1; }
echo "rows total: $total, missing runTs: $missing"
[ "$APPLY" = "--apply" ] || { echo "dry-run — pass --apply to write"; exit 0; }

# HONESTY NOTE (Wren, 2026-08-20): dcterms:created is WRITE time, not RUN
# time — they diverge whenever posting lags the suite (#3941's land posted
# 20min late). Backfilled runTs is therefore an UPPER BOUND on run time,
# accurate to the posting lag. Going forward the writer clocks runTs at RUN
# START (threaded run_epoch_ms), so only pre-backfill rows carry the skew.
# COPY, not parse (Wren's catch, verified by property census 2026-08-20):
# `name` is NOT stored on these rows — the only universal clock is
# dcterms:created, present 137,184/137,184. runTs := dcterms:created, one
# idempotent INSERT touching only rows missing runTs.
UPDATE='PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX dcterms: <http://purl.org/dc/terms/>
WITH <urn:chorus:domains:tests>
INSERT { ?r chorus:runTs ?iso }
WHERE {
  ?r a chorus:TestResult ; dcterms:created ?created .
  FILTER NOT EXISTS { ?r chorus:runTs ?t }
  BIND(STR(?created) AS ?iso)
}'
curl -sf --max-time 300 -X POST --data-urlencode "update=$UPDATE" \
  ${FUSEKI_AUTH_ARGS:-} "$FUSEKI/update" && echo "backfill applied"
missing2=$(curl -sf --max-time 30 -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=$COUNT_MISSING" "$FUSEKI/query" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])")
echo "rows missing runTs after: $missing2"
