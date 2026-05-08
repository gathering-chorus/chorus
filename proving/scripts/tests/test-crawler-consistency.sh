#!/usr/bin/env bash
# test-crawler-consistency.sh — #2827 §C integration test:
# under simulated Fuseki failure, SQLite write succeeds but Fuseki write
# fails → reconciliation pass fires retry → spine event hydration.partial.divergent
# emitted with the right shape.
#
# Method:
#   1. Hydrate fixture with Fuseki pointed at a bogus port (write fails).
#   2. Assert SQLite has rows with fuseki_status='failed'.
#   3. Assert hydration.partial.divergent fired.
#   4. Re-run with real Fuseki, assert reconciliation flips status to 'wrote'.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
SCRIPT="$CHORUS_ROOT/platform/scripts/crawler-hydrate-graph.sh"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
TEST_GRAPH="urn:chorus:test-consistency-$$"
TEST_DB=$(mktemp -t crawler-consistency.XXXXXX.db)

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "SKIP: Fuseki not reachable"
  exit 0
fi

FIXTURE=$(mktemp -d -t crawler-consistency-test.XXXX)
mkdir -p "$FIXTURE/proving" "$FIXTURE/roles/silas/ontology"
echo "a" > "$FIXTURE/proving/a.txt"
echo "b" > "$FIXTURE/proving/b.txt"
cp "$CHORUS_ROOT/roles/silas/ontology/chorus.ttl" "$FIXTURE/roles/silas/ontology/chorus.ttl"

cleanup() {
  curl -s -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
    "$FUSEKI_BASE/update" >/dev/null 2>&1
  rm -rf "$FIXTURE" "$TEST_DB"
}
trap cleanup EXIT

curl -s -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
  "$FUSEKI_BASE/update" >/dev/null 2>&1

run_with_fuseki() {
  local fuseki_url="$1"
  CHORUS_ROOT="$FIXTURE" \
  HYDRATION_GRAPH="$TEST_GRAPH" \
  HYDRATION_DB="$TEST_DB" \
  TTL="$FIXTURE/roles/silas/ontology/chorus.ttl" \
  CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
  FUSEKI_UPDATE="$fuseki_url" \
  FUSEKI_BASE="${FUSEKI_BASE_OVERRIDE:-$FUSEKI_BASE}" \
  bash "$SCRIPT" >/dev/null 2>&1
}

echo "=== #2827 §C: consistency under simulated Fuseki failure ==="

# Pre-existing Fuseki ping check passes (real port 3030 is up).
# Simulate Fuseki UPDATE failure by pointing the writer at a bogus port,
# while leaving the BASE/query at the real port (so the script's startup
# ping still passes — we want UPDATE to fail mid-run, not refuse to
# start).
SPINE_PRE=$(tail -5000 ~/.chorus/chorus.log 2>/dev/null | grep -c 'hydration.partial.divergent' | tr -d '[:space:]')
SPINE_PRE="${SPINE_PRE:-0}"

run_with_fuseki "http://localhost:9999/bogus/update"

# 1. SQLite rows should exist with fuseki_status='failed'.
FAILED_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM chorus_files WHERE fuseki_status='failed';" 2>/dev/null || echo 0)
if [ "$FAILED_COUNT" -ge 2 ] 2>/dev/null; then
  p "SQLite rows mark $FAILED_COUNT records fuseki_status=failed"
else
  f "expected ≥2 rows with fuseki_status=failed, got $FAILED_COUNT"
fi

# 2. hydration.partial.divergent emitted.
SPINE_AFTER_FAIL=$(tail -5000 ~/.chorus/chorus.log 2>/dev/null | grep -c 'hydration.partial.divergent' | tr -d '[:space:]')
SPINE_AFTER_FAIL="${SPINE_AFTER_FAIL:-0}"
DELTA=$((SPINE_AFTER_FAIL - SPINE_PRE))
if [ "$DELTA" -ge 1 ] 2>/dev/null; then
  p "hydration.partial.divergent fired ($DELTA new event(s))"
else
  f "expected hydration.partial.divergent event, got delta=$DELTA"
fi

# 3. Re-run with real Fuseki — reconciliation should flip status to 'wrote'.
run_with_fuseki "$FUSEKI_BASE/update"

WROTE_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM chorus_files WHERE fuseki_status='wrote';" 2>/dev/null || echo 0)
FAILED_COUNT_AFTER=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM chorus_files WHERE fuseki_status='failed';" 2>/dev/null || echo 0)
if [ "$WROTE_COUNT" -ge 2 ] && [ "$FAILED_COUNT_AFTER" = "0" ] 2>/dev/null; then
  p "after recovery run: $WROTE_COUNT records wrote, 0 failed (retry worked)"
else
  f "expected wrote≥2 + failed=0 after recovery, got wrote=$WROTE_COUNT failed=$FAILED_COUNT_AFTER"
fi

# 4. Sanity: the recovered records exist in Fuseki.
COUNT_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT (COUNT(?f) AS ?n) WHERE {
  GRAPH <'"$TEST_GRAPH"'> {
    ?f a chorus:File .
  }
}'
GRAPH_COUNT=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$COUNT_Q" \
  "$FUSEKI_BASE/query" 2>/dev/null \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['results']['bindings'][0]['n']['value'])" 2>/dev/null || echo 0)
if [ "$GRAPH_COUNT" -ge 2 ] 2>/dev/null; then
  p "recovered records present in Fuseki ($GRAPH_COUNT chorus:File instances)"
else
  f "expected ≥2 chorus:File in Fuseki post-recovery, got $GRAPH_COUNT"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
