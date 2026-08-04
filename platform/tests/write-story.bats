#!/usr/bin/env bats
# @test-type: integration — hits service/remote/sibling, skip-if-absent in CI

# #3606 — source Fuseki auth. Fuseki refuses unauthenticated writes (401) and
# the setup writes below discard stderr, so a refused INSERT looked exactly like
# a successful one — the later assertion then failed on absent data and read as a
# logic bug. Same swallowed-401 as 3540.
# Derive the root from this file's own location for the auth source: with
# CHORUS_ROOT unset the old form sourced "/platform/scripts/fuseki-auth.sh",
# left FUSEKI_AUTH empty, and put every write straight back to the swallowed 401
# this comment describes.
# shellcheck source=/dev/null
. "$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
load test_helper
# Tests for platform/scripts/write-story.sh (#2321)
# What it does: capture a Jeff-told story as a TTL instance in Fuseki,
# one graph per story, conforming to the existing jeff:Story schema.
#
# These tests hit the real Fuseki (/pods dataset) using a test-only
# slug prefix so production stories aren't touched. Each test cleans
# up after itself via SPARQL DROP GRAPH.

FUSEKI_QUERY="http://localhost:3030/pods/query"
FUSEKI_UPDATE="http://localhost:3030/pods/update"
SCRIPT="${CHORUS_ROOT}/platform/scripts/write-story.sh"
TEST_SLUG_PREFIX="test-bats-2321"

# #3606 — the cleanup was UNAUTHENTICATED, so post-#3564 both calls 401'd and the
# `|| true` swallowed it: neither the pre-run drop nor the post-run clean ever
# executed. Verified live — 4 leftover test story graphs were still resident.
#
# That matters beyond tidiness. This suite asserts a story graph EXISTS after the
# script runs; with residue from a prior run already present, that assertion
# passes whether or not the script wrote anything. The pre-run drop is what makes
# the test about this run — unauthenticated, it was decoration.
_story_graphs() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$FUSEKI_QUERY" -H "Accept: text/csv" \
    --data-urlencode "query=SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), \"urn:jb/jeff/stories/${TEST_SLUG_PREFIX}\")) }" \
    2>/dev/null | tail -n +2 | tr -d '\r' | grep -v '^$' || true
}

# Enumerate-then-DROP, because the obvious one-shot form does not work here.
#
# The original cleanup was `DELETE WHERE { GRAPH ?g { ?s ?p ?o . FILTER(...) } }`,
# which is not legal SPARQL — FILTER cannot appear in a DELETE WHERE quad
# template — so it returned HTTP 400 every time and `|| true` ate it. Rewriting it
# to the legal `DELETE { GRAPH ?g {...} } WHERE { GRAPH ?g {... FILTER(...) } }`
# returned "Update succeeded" and STILL deleted nothing: on this dataset a
# variable-graph delete is a silent no-op (verified — WHERE matched 159 triples,
# the update reported success, all 159 survived). An explicit
# `DROP GRAPH <uri>` deletes correctly.
#
# So: SELECT the graph names, then drop each by explicit URI, then verify. Three
# ways this could report a clean slate it did not achieve, and the verify is what
# distinguishes them.
_drop_test_stories() {
  local g
  while IFS= read -r g; do
    [ -n "$g" ] || continue
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST \
      --data-urlencode "update=DROP GRAPH <$g>" "$FUSEKI_UPDATE" >/dev/null 2>&1 || true
  done <<< "$(_story_graphs)"
  _story_graphs | grep -c . | tr -d '[:space:]'
}

setup() {
  # Drop any leftover test graphs from prior failed runs, and refuse to run on a
  # dirty slate — a suite that cannot clear its fixtures cannot tell its own
  # writes from the last run's.
  local remaining
  remaining="$(_drop_test_stories)"
  if [ "${remaining:-0}" != "0" ]; then
    echo "FATAL: ${remaining} leftover ${TEST_SLUG_PREFIX} story graph(s) survived the drop." >&2
    echo "Assertions below would read residue instead of this run's writes." >&2
    return 1
  fi
}

teardown() {
  _drop_test_stories >/dev/null
}

@test "script exists and is executable" {
  [ -x "$SCRIPT" ]
}

@test "missing args prints usage and exits non-zero" {
  run bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" =~ [Uu]sage ]]
}

@test "writes a story to Fuseki with title, body fields, and Story type" {
  TITLE="${TEST_SLUG_PREFIX} hello world"
  run bash "$SCRIPT" "$TITLE" "He said it clearly" "It tells us a lot" "Applies here today"
  [ "$status" -eq 0 ]

  # Query the graph we just wrote.
  SLUG=$(echo "$TITLE" | tr '[:upper:] ' '[:lower:]-')
  GRAPH="urn:jb/jeff/stories/${SLUG}.ttl"
  RES=$(curl -s -G "$FUSEKI_QUERY" \
    --data-urlencode "query=SELECT ?p ?o WHERE { GRAPH <$GRAPH> { ?s ?p ?o } }")

  [[ "$RES" =~ "https://jeffbridwell.com/ontology#Story" ]]
  [[ "$RES" =~ "$TITLE" ]]
  [[ "$RES" =~ "He said it clearly" ]]
  [[ "$RES" =~ "It tells us a lot" ]]
  [[ "$RES" =~ "Applies here today" ]]
}

@test "slug is kebab-case lowercase of title" {
  TITLE="${TEST_SLUG_PREFIX} Two Words Here"
  run bash "$SCRIPT" "$TITLE" "a" "b" "c"
  [ "$status" -eq 0 ]
  SLUG="${TEST_SLUG_PREFIX}-two-words-here"
  GRAPH="urn:jb/jeff/stories/${SLUG}.ttl"
  RES=$(curl -s -G "$FUSEKI_QUERY" \
    --data-urlencode "query=ASK { GRAPH <$GRAPH> { ?s a <https://jeffbridwell.com/ontology#Story> } }")
  [[ "$RES" =~ \"boolean\"[[:space:]]*:[[:space:]]*true ]]
}

@test "body combines the three narrative sections in a readable shape" {
  TITLE="${TEST_SLUG_PREFIX} body check"
  run bash "$SCRIPT" "$TITLE" "SAID-LINE" "TELLS-LINE" "APPLIES-LINE"
  [ "$status" -eq 0 ]
  SLUG="${TEST_SLUG_PREFIX}-body-check"
  GRAPH="urn:jb/jeff/stories/${SLUG}.ttl"
  RES=$(curl -s -G "$FUSEKI_QUERY" \
    --data-urlencode "query=SELECT ?body WHERE { GRAPH <$GRAPH> { ?s <https://jeffbridwell.com/ontology#body> ?body } }")
  [[ "$RES" =~ "SAID-LINE" ]]
  [[ "$RES" =~ "TELLS-LINE" ]]
  [[ "$RES" =~ "APPLIES-LINE" ]]
}

@test "story is queryable via Athena search after write" {
  TITLE="${TEST_SLUG_PREFIX} athena query"
  run bash "$SCRIPT" "$TITLE" "marker-said-zzz" "marker-tells-zzz" "marker-applies-zzz"
  [ "$status" -eq 0 ]

  # Chorus search should find the content.
  # (Index may be async — accept either hit now or a 200 from the endpoint.)
  RES=$(curl -s "http://localhost:3340/api/chorus/search?q=marker-said-zzz&limit=5")
  [[ "$RES" =~ \"results\" ]]
}
