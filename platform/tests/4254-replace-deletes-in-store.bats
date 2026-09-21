#!/usr/bin/env bats
# @test-type: integration — deploys two throwaway graphs into the werk store; needs Fuseki

# #4254 — the behaviour half of the `replace` flag, which the parser tests
# cannot reach. They prove the guard refuses the wrong manifests; these prove
# what replace actually DOES to a graph, and that merge still does not.
#
# Wren's two constraints, 2026-09-21:
#   - werk-silas, never pods. A test that replaces a graph pointed at prod is
#     the nightly-in-werk class that emptied the ontology on 08-28.
#   - no `[[` asserts — on bash 3.2 a failing `[[` that is not the last line of
#     a test passes hollow (91 of 223 suites carried that).
#
# The graphs here are minted per run and dropped in teardown, so this never
# touches a graph anyone reads.

setup() {
  ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  QUERY=http://localhost:3030/werk-silas/query
  UPDATE=http://localhost:3030/werk-silas/update
  GSP=http://localhost:3030/werk-silas/data
  STAMP="$$-${RANDOM}"
  REPLACE_GRAPH="urn:chorus:test:4254-replace-${STAMP}"
  MERGE_GRAPH="urn:chorus:test:4254-merge-${STAMP}"
  # The ONTOLOGY graph is a throwaway too. The model leg always runs, and
  # aiming it at the real one would rewrite a graph this test has no business
  # touching. It is dropped in teardown with the others.
  ONTO_GRAPH="urn:chorus:test:4254-onto-${STAMP}"
  # The TTL lives UNDER CHORUS_ROOT because manifest paths resolve against it,
  # and CHORUS_ROOT has to stay real so the model leg can find its own files.
  # TTL= is not an option: it switches the domain-set leg off entirely
  # (sets_run), which is how the first version of this test passed nothing.
  WORK="${ROOT}/platform/tests/.tmp-4254-${STAMP}"
  mkdir -p "$WORK"
  # shellcheck disable=SC1090
  . "${ROOT}/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
  DEPLOY="${ROOT}/platform/services/athena-deploy/target/debug/athena-deploy"
}

teardown() {
  curl -s -o /dev/null -X DELETE "${GSP}?graph=${REPLACE_GRAPH}" || true
  curl -s -o /dev/null -X DELETE "${GSP}?graph=${MERGE_GRAPH}" || true
  curl -s -o /dev/null -X DELETE "${GSP}?graph=${ONTO_GRAPH}" || true
  rm -rf "$WORK"
}

# Count triples on one subject in one graph. Echoes a bare integer.
subject_triples() {
  local graph="$1" subject="$2"
  curl -s -H "Accept: text/csv" \
    --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <${graph}> { <${subject}> ?p ?o } }" \
    "$QUERY" | tail -1 | tr -d '[:space:]'
}

# Write a manifest naming one set over one graph, with or without the flag.
write_manifest() {
  local file="$1" name="$2" graph="$3" ttl="$4" flag="$5"
  if test -n "$flag"; then
    printf '%s|%s|%s|%s\n' "$name" "$graph" "$ttl" "$flag" > "$file"
  else
    printf '%s|%s|%s\n' "$name" "$graph" "$ttl" > "$file"
  fi
}

# Two subjects, then one — the shape of a term being removed from source.
write_two() {
  cat > "$1" <<TTL
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix t: <urn:chorus:test:4254#> .
t:keeper a skos:Concept ; skos:prefLabel "keeper" .
t:goner  a skos:Concept ; skos:prefLabel "goner" .
TTL
}
write_one() {
  cat > "$1" <<TTL
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix t: <urn:chorus:test:4254#> .
t:keeper a skos:Concept ; skos:prefLabel "keeper" .
TTL
}

deploy_with() {
  local manifest="$1"
  CHORUS_ROOT="$ROOT" \
  DOMAIN_SET_MANIFEST="$manifest" \
  ONTOLOGY_GRAPH="$ONTO_GRAPH" \
  DEPLOY_ROLE=silas \
  FUSEKI_GSP="$GSP" FUSEKI_QUERY="$QUERY" FUSEKI_UPDATE="$UPDATE" \
    "$DEPLOY" 2>&1
}

@test "#4254 a replace set DELETES a subject the source no longer has" {
  skip_unless_store
  local dir="${WORK}/r"; mkdir -p "$dir"
  local rel="platform/tests/.tmp-4254-${STAMP}/r/set.ttl"
  write_two "${dir}/set.ttl"
  write_manifest "${dir}/m.txt" set "$REPLACE_GRAPH" "$rel" replace
  run deploy_with "${dir}/m.txt"
  echo "$output"
  test "$status" -eq 0

  test "$(subject_triples "$REPLACE_GRAPH" 'urn:chorus:test:4254#goner')" -eq 2

  write_one "${dir}/set.ttl"
  run deploy_with "${dir}/m.txt"
  echo "$output"
  test "$status" -eq 0

  # The whole point: gone from the STORE, not merely from the file.
  test "$(subject_triples "$REPLACE_GRAPH" 'urn:chorus:test:4254#goner')" -eq 0
  test "$(subject_triples "$REPLACE_GRAPH" 'urn:chorus:test:4254#keeper')" -eq 2
}

@test "#4254 NEGATIVE PROOF: a merge set KEEPS it, which is why replace exists" {
  skip_unless_store
  local dir="${WORK}/m"; mkdir -p "$dir"
  local rel="platform/tests/.tmp-4254-${STAMP}/m/set.ttl"
  write_two "${dir}/set.ttl"
  write_manifest "${dir}/m.txt" set "$MERGE_GRAPH" "$rel" ""
  run deploy_with "${dir}/m.txt"
  echo "$output"
  test "$status" -eq 0
  test "$(subject_triples "$MERGE_GRAPH" 'urn:chorus:test:4254#goner')" -eq 2

  write_one "${dir}/set.ttl"
  run deploy_with "${dir}/m.txt"
  echo "$output"
  test "$status" -eq 0

  # Survives — the #4250 ghost, reproduced deliberately. If this ever reads 0,
  # the merge changed and the replace flag is no longer buying anything.
  test "$(subject_triples "$MERGE_GRAPH" 'urn:chorus:test:4254#goner')" -eq 2
}

skip_unless_store() {
  local code
  code="$(curl -s -o /dev/null -w '%{http_code}' --data-urlencode 'query=ASK{}' "$QUERY")"
  if test "$code" != "200"; then
    skip "werk-silas not reachable (HTTP ${code}) — this suite needs the store"
  fi
  if test ! -x "$DEPLOY"; then
    skip "athena-deploy not built at ${DEPLOY}"
  fi
}
