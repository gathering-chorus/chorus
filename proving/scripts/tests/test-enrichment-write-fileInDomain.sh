#!/usr/bin/env bash
# test-enrichment-write-fileInDomain.sh — #2844: enrichment writer adds
# chorus:fileInDomain + chorus:fileHasOwner to existing chorus:File
# instances based on path heuristics.
#
# Method:
#   1. Seed a fixture-named graph with a handful of chorus:File instances
#      at known paths (using crawler-hydrate-graph.sh as the seeder).
#   2. Run the enrichment writer against that graph.
#   3. SPARQL query: assert each fixture file has the expected
#      fileInDomain + (where applicable) fileHasOwner.
#   4. Re-run, assert idempotency (no duplicate triples).

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus-werk/kade}"
ENRICH="$CHORUS_ROOT/platform/scripts/enrichment-write-fileInDomain.sh"
HYDRATE="$CHORUS_ROOT/platform/scripts/crawler-hydrate-graph.sh"
FUSEKI_BASE="${FUSEKI_BASE:-http://localhost:3030/pods}"
TEST_GRAPH="urn:chorus:test-enrichment-$$"
TEST_DB=$(mktemp -t enrich.XXXXXX.db)

if ! curl -sf --max-time 3 "http://localhost:3030/\$/ping" -o /dev/null 2>/dev/null; then
  echo "SKIP: Fuseki not reachable"
  exit 0
fi

# Fixture mimics chorus tree layout under a synthetic <root>/chorus/...
# so the strip regex (.*/chorus(-werk/<role>)?/) hits.
FIXTURE_BASE=$(mktemp -d -t enrich-test.XXXX)
FIXTURE="$FIXTURE_BASE/chorus"
mkdir -p \
  "$FIXTURE/proving/scripts/tests" \
  "$FIXTURE/platform/scripts" \
  "$FIXTURE/roles/kade" \
  "$FIXTURE/roles/silas/ontology" \
  "$FIXTURE/skills"

echo "test-a" > "$FIXTURE/proving/scripts/tests/a.sh"
echo "git-stub" > "$FIXTURE/platform/scripts/git-helper.sh"
echo "kade-state" > "$FIXTURE/roles/kade/current-work.md"
echo "skill" > "$FIXTURE/skills/foo.md"
cp "$CHORUS_ROOT/roles/silas/ontology/chorus.ttl" "$FIXTURE/roles/silas/ontology/chorus.ttl"

cleanup() {
  curl -s -X POST -H 'Content-Type: application/sparql-update' \
    --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
    "$FUSEKI_BASE/update" >/dev/null 2>&1
  rm -rf "$FIXTURE_BASE" "$TEST_DB"
}
trap cleanup EXIT

curl -s -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary "DROP SILENT GRAPH <$TEST_GRAPH>" \
  "$FUSEKI_BASE/update" >/dev/null 2>&1

echo "=== #2844 enrichment writer integration ==="

# Seed via crawler.
CHORUS_ROOT="$FIXTURE" \
HYDRATION_GRAPH="$TEST_GRAPH" \
HYDRATION_DB="$TEST_DB" \
TTL="$FIXTURE/roles/silas/ontology/chorus.ttl" \
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
bash "$HYDRATE" >/dev/null 2>&1

SEED_COUNT=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode 'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT (COUNT(?f) AS ?n) WHERE { GRAPH <'"$TEST_GRAPH"'> { ?f a chorus:File } }' \
  "$FUSEKI_BASE/query" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['results']['bindings'][0]['n']['value'])" 2>/dev/null || echo 0)

if [ "$SEED_COUNT" -ge 5 ] 2>/dev/null; then
  p "seed: $SEED_COUNT chorus:File instances hydrated"
else
  f "seed failed: only $SEED_COUNT chorus:File instances"
fi

# Run enrichment writer.
CHORUS_ROOT="$FIXTURE" \
HYDRATION_GRAPH="$TEST_GRAPH" \
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
bash "$ENRICH" 2>&1 | tail -1

# Each fixture file: assert correct fileInDomain.
check_predicate() {
  local path_substr="$1" expected_subdomain="$2"
  local q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
ASK { GRAPH <'"$TEST_GRAPH"'> {
  ?f chorus:filePath ?p ; chorus:fileInDomain chorus:'"$expected_subdomain"' .
  FILTER(CONTAINS(?p, "'"$path_substr"'"))
} }'
  local resp
  resp=$(curl -s -G -H 'Accept: application/sparql-results+json' \
    --data-urlencode "query=$q" "$FUSEKI_BASE/query" 2>/dev/null)
  if echo "$resp" | grep -qE '"boolean"[[:space:]]*:[[:space:]]*true'; then
    p "$path_substr → chorus:$expected_subdomain"
  else
    f "expected $path_substr → chorus:$expected_subdomain, ASK returned: $resp"
  fi
}

check_predicate "proving/scripts/tests/a.sh" "tests-domain"
check_predicate "platform/scripts/git-helper.sh" "version-control-domain"
check_predicate "roles/kade/current-work.md" "roles-domain"
check_predicate "skills/foo.md" "skills-service"

# Owner check on the kade-path file.
OWNER_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
ASK { GRAPH <'"$TEST_GRAPH"'> {
  ?f chorus:filePath ?p ; chorus:fileHasOwner chorus:kade .
  FILTER(CONTAINS(?p, "roles/kade/current-work.md"))
} }'
OWNER_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$OWNER_Q" "$FUSEKI_BASE/query" 2>/dev/null)
if echo "$OWNER_RESP" | grep -qE '"boolean"[[:space:]]*:[[:space:]]*true'; then
  p "roles/kade/* → chorus:fileHasOwner chorus:kade"
else
  f "expected fileHasOwner=kade for kade path, ASK returned: $OWNER_RESP"
fi

# Idempotency: re-run, assert each file still has exactly one fileInDomain.
CHORUS_ROOT="$FIXTURE" \
HYDRATION_GRAPH="$TEST_GRAPH" \
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log" \
bash "$ENRICH" >/dev/null 2>&1

DUP_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?f (COUNT(?d) AS ?n) WHERE {
  GRAPH <'"$TEST_GRAPH"'> { ?f chorus:fileInDomain ?d }
} GROUP BY ?f HAVING (?n > 1)'
DUP_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$DUP_Q" "$FUSEKI_BASE/query" 2>/dev/null)
DUP_COUNT=$(echo "$DUP_RESP" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d['results']['bindings']))" 2>/dev/null || echo "?")
if [ "$DUP_COUNT" = "0" ]; then
  p "idempotency: no file has duplicate fileInDomain triples after second run"
else
  f "idempotency broken: $DUP_COUNT files have multiple fileInDomain triples"
fi

# Spine event check.
SPINE=$(tail -2000 ~/.chorus/chorus.log 2>/dev/null | grep -c 'enrichment.fileInDomain.written' | tr -d '[:space:]')
SPINE="${SPINE:-0}"
if [ "$SPINE" -ge 1 ] 2>/dev/null; then
  p "enrichment.fileInDomain.written event(s) emitted ($SPINE in tail)"
else
  f "expected enrichment.fileInDomain.written event"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
