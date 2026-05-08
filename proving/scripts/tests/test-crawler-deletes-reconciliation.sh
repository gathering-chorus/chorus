#!/usr/bin/env bash
# test-crawler-deletes-reconciliation.sh — #2827 §D integration test:
# hydrate a fixture, delete a file, re-crawl, assert chorus:stale flag
# lands and crawler.graph.orphan.detected fires. Then restore the file,
# re-crawl, assert the flag clears (visibility-not-removal works both ways).

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
SCRIPT="$CHORUS_ROOT/platform/scripts/crawler-hydrate-graph.sh"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
TEST_GRAPH="urn:chorus:test-deletes-$$"

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "SKIP: Fuseki not reachable"
  exit 0
fi

FIXTURE=$(mktemp -d -t crawler-deletes-test.XXXX)
mkdir -p "$FIXTURE/proving" "$FIXTURE/roles/silas/ontology"
echo "stable" > "$FIXTURE/proving/stable.txt"
echo "doomed" > "$FIXTURE/proving/doomed.txt"
cp "$CHORUS_ROOT/roles/silas/ontology/chorus.ttl" "$FIXTURE/roles/silas/ontology/chorus.ttl"

cleanup() {
  curl -s -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
    "$FUSEKI_BASE/update" >/dev/null 2>&1
  rm -rf "$FIXTURE"
}
trap cleanup EXIT

curl -s -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
  "$FUSEKI_BASE/update" >/dev/null 2>&1

run_crawler() {
  CHORUS_ROOT="$FIXTURE" \
  HYDRATION_GRAPH="$TEST_GRAPH" \
  TTL="$FIXTURE/roles/silas/ontology/chorus.ttl" \
  CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
  bash "$SCRIPT" >/dev/null 2>&1
}

count_stale() {
  local q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT (COUNT(?f) AS ?n) WHERE {
  GRAPH <'"$TEST_GRAPH"'> {
    ?f a chorus:File ; chorus:stale true .
  }
}'
  curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$q" \
    "$FUSEKI_BASE/query" 2>/dev/null \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['results']['bindings'][0]['n']['value'])" 2>/dev/null || echo "?"
}

echo "=== #2827 §D: deletes reconciliation ==="

# Run 1: hydrate. No orphans expected.
run_crawler
STALE_PRE=$(count_stale)
if [ "$STALE_PRE" = "0" ]; then
  p "post-hydration: 0 stale instances (no orphans yet)"
else
  f "expected 0 stale post-hydration, got $STALE_PRE"
fi

# Capture spine baseline.
SPINE_PRE=$(tail -300 ~/.chorus/chorus.log 2>/dev/null | grep -c 'crawler.graph.orphan.detected' | tr -d '[:space:]')
SPINE_PRE="${SPINE_PRE:-0}"

# Delete one fixture file.
rm "$FIXTURE/proving/doomed.txt"

# Run 2: re-crawl. doomed.txt should now be orphan.
run_crawler

STALE_POST=$(count_stale)
if [ "$STALE_POST" = "1" ]; then
  p "after delete + re-crawl: 1 stale instance (doomed.txt)"
else
  f "expected 1 stale instance, got $STALE_POST"
fi

# Verify the stale instance is the doomed file.
DOOMED_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
ASK { GRAPH <'"$TEST_GRAPH"'> { ?f chorus:filePath ?p ; chorus:stale true . FILTER(CONTAINS(?p, "doomed.txt")) } }'
ASK_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$DOOMED_Q" "$FUSEKI_BASE/query" 2>/dev/null)
if echo "$ASK_RESP" | grep -qE '"boolean"[[:space:]]*:[[:space:]]*true'; then
  p "stale flag landed on the deleted file (doomed.txt)"
else
  f "expected stale=true on doomed.txt, ASK returned: $ASK_RESP"
fi

SPINE_POST=$(tail -300 ~/.chorus/chorus.log 2>/dev/null | grep -c 'crawler.graph.orphan.detected' | tr -d '[:space:]')
SPINE_POST="${SPINE_POST:-0}"
DELTA=$((SPINE_POST - SPINE_PRE))
if [ "$DELTA" -ge 1 ] 2>/dev/null; then
  p "crawler.graph.orphan.detected fired ($DELTA new event(s))"
else
  f "expected crawler.graph.orphan.detected event, got delta=$DELTA"
fi

# Run 3: restore the file, re-crawl, assert stale is cleared.
echo "doomed-restored" > "$FIXTURE/proving/doomed.txt"
run_crawler
STALE_RESTORED=$(count_stale)
if [ "$STALE_RESTORED" = "0" ]; then
  p "after restore + re-crawl: stale flag cleared (visibility-not-removal works both ways)"
else
  f "expected 0 stale after restore, got $STALE_RESTORED"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
