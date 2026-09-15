#!/usr/bin/env bash
# test-enrichment-write-fileInDomain.sh — #2844: enrichment writer adds
# chorus:fileInDomain + chorus:fileHasOwner to existing chorus:File
# instances based on path heuristics.
#
# Method:
#   1. Seed a fixture-named graph with a handful of chorus:File instances
#      at known paths (seeded inline — #4173 retired the crawler script).
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

# The five paths BELONGS_MAP actually carries. #3021 narrowed this writer from
# a function-based scan of every File row to five TARGETED spine files, and this
# fixture was never updated: it kept building a.sh / git-helper.sh / foo.md and
# asserting domains the map no longer contains, so the suite could not pass. It
# never reported that, because a proving/ suite was not selected by any diff
# until #4173 made a changed suite run itself.
BELONGS_RELS=(
  "platform/api/src/spine-event-write.ts"
  "platform/api/tests/spine-event-endpoint.integration.test.ts"
  "platform/api/tests/spine-event-write.test.ts"
  "platform/tests/spine-emit-drift-audit.bats"
  "platform/tests/spine-tick-poller-inject-resolve.bats"
)
for rel in "${BELONGS_RELS[@]}"; do
  mkdir -p "$FIXTURE/$(dirname "$rel")"
  echo "fixture" > "$FIXTURE/$rel"
done

# The writer reads CHORUS_ROOT for TWO unrelated things: the tree whose paths it
# strips, and where its store credential lives. This suite points CHORUS_ROOT at
# the fixture for the first, which silently removed the second — every write
# 401'd and the run reported "5 files tagged (1 batch failure)". The fixture
# carries the credential so the writer can reach the store it is being tested
# against. The double duty of CHORUS_ROOT is the writer's defect, noted here.
mkdir -p "$FIXTURE/platform/scripts"
cp "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh" "$FIXTURE/platform/scripts/fuseki-auth.sh"

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

# Seed the fixture directly.
#
# #4173 retired the crawler shell script this test borrowed as a seeder.
# Borrowing a walker to set up a different subject's test was always the wrong
# coupling — it made this suite fail whenever the walker changed, for reasons
# that had nothing to do with enrichment. The replacement, chorus-crawl, writes
# through the generated door and cannot target an arbitrary test graph by
# design, so the fixture seeds itself: the rows are the input to the thing under
# test, and stating them plainly is clearer than producing them.
SEED_INSERT="PREFIX chorus: <https://jeffbridwell.com/chorus#> INSERT DATA { GRAPH <$TEST_GRAPH> {"
# EXACTLY the five fixture files created above, at the paths the strip regex
# produces. The first cut seeded a plausible-looking list of real repo paths
# instead, so the seed passed its own count check and every assertion below
# failed looking for rows that were never there.
# filePath is the FIXTURE-ABSOLUTE path, because the writer matches on
# STRENDS(?p, "/<rel>") — a bare relative path ends with the rel but not with
# "/<rel>", so a seed of relative paths matches nothing and tags 0 files.
for rel in "${BELONGS_RELS[@]}"; do
  uri="https://jeffbridwell.com/chorus#file-$(printf '%s' "$rel" | tr -c 'a-zA-Z0-9' '-')"
  SEED_INSERT="$SEED_INSERT <$uri> a chorus:File ; chorus:filePath \"$FIXTURE/$rel\" ."
done
SEED_INSERT="$SEED_INSERT } }"
# #3566 — Fuseki 401s a bare write. The first cut of this seed sent the INSERT
# with no credential AND swallowed the response into /dev/null, so a refused
# write looked identical to a successful one and only the count check three
# lines later said anything. Carry the credential, and let the status code be
# seen: a seed that cannot write must say so itself.
source "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh"
SEED_CODE=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /dev/null -w '%{http_code}' \
  -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary "$SEED_INSERT" "$FUSEKI_BASE/update")
case "$SEED_CODE" in
  2*) ;;
  *) f "seed write refused by the store — HTTP $SEED_CODE" ;;
esac

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

# The writer's real contract: the five BELONGS_MAP files carry chorus:spine.
check_predicate "platform/api/src/spine-event-write.ts" "spine"
check_predicate "platform/api/tests/spine-event-write.test.ts" "spine"
check_predicate "platform/tests/spine-emit-drift-audit.bats" "spine"

# NEGATIVE PROOF (#3734): the checks above pass for every file if the writer
# tags indiscriminately. A file that is NOT in BELONGS_MAP must come back
# untagged, or this suite cannot tell "tagged correctly" from "tagged".
UNMAPPED="$FIXTURE/platform/scripts/not-in-the-map.sh"
mkdir -p "$(dirname "$UNMAPPED")"; echo "fixture" > "$UNMAPPED"
NEG_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
ASK { GRAPH <'"$TEST_GRAPH"'> {
  ?f chorus:filePath ?p ; chorus:fileInDomain ?d .
  FILTER(CONTAINS(?p, "not-in-the-map.sh"))
} }'
NEG_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$NEG_Q" "$FUSEKI_BASE/query" 2>/dev/null)
if echo "$NEG_RESP" | grep -qE '"boolean"[[:space:]]*:[[:space:]]*false'; then
  p "NEGATIVE PROOF: a file outside BELONGS_MAP is not tagged"
else
  f "a file outside BELONGS_MAP was tagged — the checks above prove nothing: $NEG_RESP"
fi

# Owner check on the kade-path file.
OWNER_Q='PREFIX chorus: <https://jeffbridwell.com/chorus#>
ASK { GRAPH <'"$TEST_GRAPH"'> {
  ?f chorus:filePath ?p ; chorus:fileHasOwner chorus:role-wren .
  FILTER(CONTAINS(?p, "spine-event-write.ts"))
} }'
OWNER_RESP=$(curl -s -G -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=$OWNER_Q" "$FUSEKI_BASE/query" 2>/dev/null)
if echo "$OWNER_RESP" | grep -qE '"boolean"[[:space:]]*:[[:space:]]*true'; then
  p "spine files → chorus:fileHasOwner chorus:role-wren"
else
  f "expected fileHasOwner=role-wren for a spine file, ASK returned: $OWNER_RESP"
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
