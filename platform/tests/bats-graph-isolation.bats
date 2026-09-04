#!/usr/bin/env bats
# @test-type: unit (naming) + integration (residue scan; skip-if-absent)
# Subject: throwaway graphs that suites write into the shared store.
#
# 2026-09-04: 77 graphs named urn:chorus:ontology-test-bats-3540-<pid> were
# sitting in the production store. Cause: the per-process `$$` suffix. bats runs
# setup_file, every @test, and teardown_file in separate processes, so the file
# body is re-sourced and $$ differs each time — teardown drops a name that was
# never created, and every per-test graph is left behind. Two suites
# (products-3603-migration, source-exclusivity-3732) went red the next night
# reading that residue.
#
# The name has to be unique per RUN and identical across the run's processes.
load test_helper

WRITERS=(
  "3540-tests-domain-schema-land.bats"
  "4029-deploy-twice-same-count.bats"
)

@test "no store-writing suite derives its graph name from the process id" {
  for f in "${WRITERS[@]}"; do
    run grep -nE '^(TEST_GRAPH|GRAPH)=.*\$\$' "$BATS_TEST_DIRNAME/$f"
    [ "$status" -ne 0 ] || {
      echo "$f still names its graph from \$\$: $output"
      false
    }
  done
}

@test "store-writing suites take the shared run-scoped name" {
  for f in "${WRITERS[@]}"; do
    grep -qE '^(TEST_GRAPH|GRAPH)="?\$\(test_graph_name ' "$BATS_TEST_DIRNAME/$f"
  done
}

@test "the run-scoped name is identical across processes" {
  # the property teardown_file depends on, asserted from two real subshells
  local a b
  a="$(bash -c "source '$BATS_TEST_DIRNAME/test_helper.bash'; test_graph_name 3540")"
  b="$(bash -c "source '$BATS_TEST_DIRNAME/test_helper.bash'; test_graph_name 3540")"
  [ "$a" = "$b" ]
  [ -n "$a" ]
}

@test "NEGATIVE PROOF: the \$\$ name differs across the same two processes" {
  # Without this, the test above is green for a name that is merely constant —
  # including one that is constant ACROSS RUNS, which is the collision the $$
  # suffix was added to fix. Show the two states apart.
  local a b
  a="$(bash -c 'echo "urn:chorus:ontology-test-bats-3540-$$"')"
  b="$(bash -c 'echo "urn:chorus:ontology-test-bats-3540-$$"')"
  [ "$a" != "$b" ]
}

@test "the run-scoped name is unique per run" {
  local a b
  a="$(BATS_RUN_TMPDIR=/tmp/bats-run-AAAA bash -c "source '$BATS_TEST_DIRNAME/test_helper.bash'; test_graph_name 3540")"
  b="$(BATS_RUN_TMPDIR=/tmp/bats-run-BBBB bash -c "source '$BATS_TEST_DIRNAME/test_helper.bash'; test_graph_name 3540")"
  [ "$a" != "$b" ]
}

@test "the live store carries no leftover bats test graphs" {
  # The residue itself — the thing Jeff pays for. Skip when the store is absent
  # (this runs on boxes without it), never pass vacuously when it is present.
  curl -sf --max-time 5 "http://localhost:3030/$/ping" >/dev/null 2>&1 || skip "fuseki not reachable"
  # shellcheck source=/dev/null
  . "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  local n
  n="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "http://localhost:3030/pods/query" \
        --data-urlencode 'query=SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), "urn:chorus:ontology-test-bats-")) }' \
        -H "Accept: text/csv" 2>/dev/null | tail -1 | tr -d '[:space:]')"
  [ -n "$n" ] || skip "store did not answer the residue query"
  [ "$n" = "0" ] || {
    echo "$n leftover urn:chorus:ontology-test-bats-* graphs in the live store"
    false
  }
}

@test "NEGATIVE PROOF: the residue scan sees a leftover graph when one exists" {
  # The scan above reports 0 on a clean store. A scan that reports 0 because it
  # cannot see graphs at all reports the same thing. Mint one leftover, watch
  # the count go non-zero, drop it.
  curl -sf --max-time 5 "http://localhost:3030/$/ping" >/dev/null 2>&1 || skip "fuseki not reachable"
  # shellcheck source=/dev/null
  . "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  local g; g="$(test_graph_name residue-proof)"
  local code
  code="$(curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
    -X POST "http://localhost:3030/pods/update" \
    --data-urlencode "update=INSERT DATA { GRAPH <$g> { <urn:chorus:probe> <urn:chorus:at> \"residue\" } }" 2>/dev/null)"
  case "$code" in 2*) : ;; *) skip "store refused the fixture write (HTTP $code)" ;; esac

  local n
  n="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "http://localhost:3030/pods/query" \
        --data-urlencode 'query=SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } FILTER(STRSTARTS(STR(?g), "urn:chorus:ontology-test-bats-")) }' \
        -H "Accept: text/csv" 2>/dev/null | tail -1 | tr -d '[:space:]')"

  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "http://localhost:3030/pods/data?graph=$g" -o /dev/null 2>/dev/null

  [ "$n" != "0" ]
  [ -n "$n" ]
}
