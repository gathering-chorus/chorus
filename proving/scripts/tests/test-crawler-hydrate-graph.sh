#!/usr/bin/env bash
# test-crawler-hydrate-graph.sh — #2827 §B integration test: hydrate a fixture
# filesystem into a fixture-named Fuseki graph, assert SPARQL query returns
# the expected chorus:File triples, clean up.
#
# Uses a NAMED graph (urn:chorus:test-hydration-${PID}) so the test never
# touches the live urn:chorus:instances data. Drops the test graph at exit.
#
# Skipped if Fuseki isn't running locally — the test asserts behavior
# against a real Fuseki, not a mock; if the substrate isn't up the test
# can't honestly verify.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
SCRIPT="$CHORUS_ROOT/platform/scripts/crawler-hydrate-graph.sh"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
TEST_GRAPH="urn:chorus:test-hydration-$$"

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "SKIP: Fuseki not reachable at localhost:3030 — integration test requires real substrate"
  exit 0
fi

if [ ! -x "$SCRIPT" ]; then
  echo "FAIL: $SCRIPT not found / not executable"
  exit 1
fi

FIXTURE=$(mktemp -d -t crawler-hydrate-test.XXXX)
mkdir -p "$FIXTURE/proving" "$FIXTURE/platform/scripts" "$FIXTURE/roles/silas/ontology"

echo "kade test 1" > "$FIXTURE/proving/file1.txt"
echo "kade test 2" > "$FIXTURE/platform/scripts/file2.sh"
echo "kade test 3" > "$FIXTURE/roles/silas/ontology/note.md"

# Need a real chorus.ttl-like file so the script can resolve the registry.
# Minimal viable: copy the canonical's hydration registry section.
cp "$CHORUS_ROOT/roles/silas/ontology/chorus.ttl" "$FIXTURE/roles/silas/ontology/chorus.ttl"

cleanup() {
  curl -s -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "DROP GRAPH <$TEST_GRAPH>" \
    "$FUSEKI_BASE/update" >/dev/null 2>&1
  rm -rf "$FIXTURE"
}
trap cleanup EXIT

# Pre-clean: make sure no leftover from a prior failed run.
curl -s -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
  "$FUSEKI_BASE/update" >/dev/null 2>&1

echo "=== #2827 §B integration: hydrate fixture filesystem ==="
echo "Fixture: $FIXTURE"
echo "Test graph: $TEST_GRAPH"

# Run the crawler against the fixture, writing into the test graph.
# Pin CHORUS_LOG to the real chorus-log so spine events still emit.
CHORUS_ROOT="$FIXTURE" \
HYDRATION_GRAPH="$TEST_GRAPH" \
TTL="$FIXTURE/roles/silas/ontology/chorus.ttl" \
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
bash "$SCRIPT" 2>&1 | head -10
RC=$?

if [ "$RC" -eq 0 ]; then
  p "crawler-hydrate-graph rc=0"
else
  f "crawler-hydrate-graph rc=$RC"
fi

# Query SPARQL to count chorus:File instances in the test graph.
COUNT_QUERY='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT (COUNT(?f) AS ?n) WHERE {
  GRAPH <'"$TEST_GRAPH"'> {
    ?f a chorus:File .
  }
}'

RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$COUNT_QUERY" \
  "$FUSEKI_BASE/query" 2>/dev/null)
COUNT=$(echo "$RESP" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['results']['bindings'][0]['n']['value'])" 2>/dev/null || echo "?")

echo "  chorus:File instances in test graph: $COUNT"
# Fixture has 4 files: 3 explicit fixtures + the chorus.ttl copy needed
# for the registry resolver.
if [ "$COUNT" = "4" ]; then
  p "SPARQL count: 4 chorus:File instances (3 fixtures + chorus.ttl copy)"
else
  f "expected 4 chorus:File instances, got $COUNT"
fi

# Verify path predicate landed for one of the files.
PATH_QUERY='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?p WHERE {
  GRAPH <'"$TEST_GRAPH"'> {
    ?f a chorus:File ; chorus:filePath ?p .
    FILTER(CONTAINS(?p, "file1.txt"))
  }
}'
PATH_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$PATH_QUERY" \
  "$FUSEKI_BASE/query" 2>/dev/null)
PATH_FOUND=$(echo "$PATH_RESP" | python3 -c "import json,sys; d=json.load(sys.stdin); b=d['results']['bindings']; print(b[0]['p']['value'] if b else '')" 2>/dev/null || echo "")

if [ -n "$PATH_FOUND" ] && [[ "$PATH_FOUND" == *"file1.txt"* ]]; then
  p "filePath predicate landed for fixture file (path=$PATH_FOUND)"
else
  f "expected file1.txt path in graph, got: $PATH_FOUND"
fi

# Idempotency: run again, count should still be 4 (replace-on-write).
echo "  Re-running for idempotency check..."
CHORUS_ROOT="$FIXTURE" \
HYDRATION_GRAPH="$TEST_GRAPH" \
TTL="$FIXTURE/roles/silas/ontology/chorus.ttl" \
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
bash "$SCRIPT" >/dev/null 2>&1

RESP2=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$COUNT_QUERY" \
  "$FUSEKI_BASE/query" 2>/dev/null)
COUNT2=$(echo "$RESP2" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['results']['bindings'][0]['n']['value'])" 2>/dev/null || echo "?")

if [ "$COUNT2" = "4" ]; then
  p "idempotency: count still 4 after second run (replace-on-write)"
else
  f "idempotency broken: expected 4 after second run, got $COUNT2"
fi

# Spine event check: crawler.graph.hydrated should have fired with class=chorus:File
SPINE_HIT=$(tail -200 ~/.chorus/chorus.log 2>/dev/null | grep -c 'crawler.graph.hydrated' | tr -d '[:space:]')
SPINE_HIT="${SPINE_HIT:-0}"
if [ "$SPINE_HIT" -gt 0 ] 2>/dev/null; then
  p "spine emitted crawler.graph.hydrated ($SPINE_HIT events in tail)"
else
  f "expected crawler.graph.hydrated event in spine, found none (count=$SPINE_HIT)"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
