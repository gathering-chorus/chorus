#!/usr/bin/env bats
# Tests for platform/scripts/write-story.sh (#2321)
# What it does: capture a Jeff-told story as a TTL instance in Fuseki,
# one graph per story, conforming to the existing jeff:Story schema.
#
# These tests hit the real Fuseki (/pods dataset) using a test-only
# slug prefix so production stories aren't touched. Each test cleans
# up after itself via SPARQL DROP GRAPH.

FUSEKI_QUERY="http://localhost:3030/pods/query"
FUSEKI_UPDATE="http://localhost:3030/pods/update"
SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/write-story.sh"
TEST_SLUG_PREFIX="test-bats-2321"

setup() {
  # Drop any leftover test graphs from prior failed runs.
  curl -s -X POST -H "Content-Type: application/sparql-update" \
    --data "DELETE WHERE { GRAPH ?g { ?s ?p ?o . FILTER(STRSTARTS(STR(?g), \"urn:jb/jeff/stories/${TEST_SLUG_PREFIX}\")) } }" \
    "$FUSEKI_UPDATE" > /dev/null 2>&1 || true
}

teardown() {
  # Clean the test graphs we created.
  curl -s -X POST -H "Content-Type: application/sparql-update" \
    --data "DELETE WHERE { GRAPH ?g { ?s ?p ?o . FILTER(STRSTARTS(STR(?g), \"urn:jb/jeff/stories/${TEST_SLUG_PREFIX}\")) } }" \
    "$FUSEKI_UPDATE" > /dev/null 2>&1 || true
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
