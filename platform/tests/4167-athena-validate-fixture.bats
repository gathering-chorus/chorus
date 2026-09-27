#!/usr/bin/env bats
# @test-type: integration — writes and drops its OWN throwaway graph; never the live ones.
# @domain: knowledge — the product domain this suite guards (#4334)
#
# #4167 AC7 — the check can reach BOTH states.
#
# The whole reason this card exists is that the bash sweep could print PROVEN
# CLEAN against a store it never reached: every check treated a failed query as
# zero rows. A rewrite that is only ever run against the live graph proves
# nothing about that, because the live graph is always dirty — a check that is
# permanently red is as useless as one that is permanently green, and neither
# tells you the check works.
#
# So: build a graph with a KNOWN dangling edge and a KNOWN untyped subject, and
# require the verb to report them. Then build a clean one and require it to say
# clean. Then take the store away and require UNMEASURED rather than either.

BIN="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/services/athena-validate/target/release/athena-validate"
NS="https://jeffbridwell.com/chorus#"
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

load test_helper   # test_graph_name — run-scoped throwaway graph

setup_file() {
  # shellcheck source=/dev/null
  . "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
  # The graph name comes from the shared helper, not from $$.
  #
  # Two reasons, both learned the hard way by other suites. $$ is per PROCESS
  # and bats runs setup_file, each test and teardown_file separately, so a
  # $$-named graph is created under one pid and dropped under another — 77
  # leaked graphs were found in the live store on 2026-09-04 that way. And the
  # `urn:chorus:ontology-test-bats-` prefix is the sanctioned throwaway
  # namespace; my first version wrote `urn:chorus:bats-4167-…`, which is a name
  # nothing recognises as a fixture, so the membrane saw a test writing an
  # unknown production surface and said so (two membrane.violation events,
  # 2026-09-19 18:49). The membrane was right and the test was wrong.
  export FIXTURE_GRAPH="$(test_graph_name 4167)"
  # #4332 — the fixture goes to the test dataset, never /pods (lib/test-store.sh)
  . "$BATS_TEST_DIRNAME/lib/test-store.sh"
  test_store || skip "UNMEASURED: $TEST_STORE_WHY"
  export UPD="$FUSEKI_UPDATE"
  export QRY="$FUSEKI_QUERY"
}

teardown_file() {
  # #4175 — every Fuseki write carries a timeout, including a teardown: an
  # unbounded one hangs the whole run when the store's write lock is held.
  curl -s --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPD" \
    --data-urlencode "update=DROP SILENT GRAPH <$FIXTURE_GRAPH>" >/dev/null 2>&1 || true
}

# FUSEKI_AUTH is a bash ARRAY, and arrays do not survive `export` into the
# per-test subshells bats runs. The first version of this file relied on
# setup_file exporting it, and every write came back curl 22 — an HTTP error
# that looks exactly like a broken fixture rather than a missing credential.
# Each helper sources the auth itself.
_auth() { . "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true; }

_load_dirty() {
  _auth
  curl -sf --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPD" --data-urlencode \
    "update=DROP SILENT GRAPH <$FIXTURE_GRAPH> ;
     INSERT DATA { GRAPH <$FIXTURE_GRAPH> {
       <${NS}fixture-row-a> a <${NS}Skill> ; <${NS}hasDomain> <${NS}fixture-nowhere> .
       <${NS}fixture-row-b> <${NS}label> \"no type at all\" .
     } }"
}

_load_clean() {
  _auth
  curl -sf --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$UPD" --data-urlencode \
    "update=DROP SILENT GRAPH <$FIXTURE_GRAPH> ;
     INSERT DATA { GRAPH <$FIXTURE_GRAPH> {
       <${NS}fixture-row-c> a <${NS}Skill> ; <${NS}label> \"complete and connected\" .
     } }"
}

@test "the binary exists — a missing binary must not read as a clean sweep" {
  [ -x "$BIN" ]
}

@test "NEGATIVE PROOF: a graph with a known dangling edge and a known untyped row is reported" {
  _load_dirty
  run env FUSEKI_QUERY="$QRY" "$BIN"
  # The dangling edge: hasDomain points at fixture-nowhere, which is never a subject.
  echo "$output" | grep -q "graph-issue|dangling-edge|fixture-row-a"
  # The untyped subject: fixture-row-b carries data and no rdf:type.
  echo "$output" | grep -q "graph-issue|untyped-instance|fixture-row-b"
}

@test "NEGATIVE PROOF: the same two violations are ABSENT once the fixture is clean" {
  _load_clean
  run env FUSEKI_QUERY="$QRY" "$BIN"
  test -z "$(printf '%s' "$output" | grep -F "dangling-edge|fixture-row-a" || true)"
  test -z "$(printf '%s' "$output" | grep -F "untyped-instance|fixture-row-b" || true)"
}

@test "NEGATIVE PROOF: an unreachable store is UNMEASURED and exit 2, never a count" {
  run env FUSEKI_QUERY="http://127.0.0.1:9/query" CHORUS_OWL_API="http://127.0.0.1:9" "$BIN"
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "graph-summary|UNMEASURED|unreachable"
  test -z "$(printf '%s' "$output" | grep -F "|clean" || true)"
}

@test "the report format is unchanged — graph-issue lines and one graph-summary" {
  # #4332 — the dirty fixture, so an issue line exists to check. This case
  # used to load the clean fixture and lean on /pods always having issues;
  # in the test dataset the clean fixture is the whole store.
  _load_dirty
  run env FUSEKI_QUERY="$QRY" "$BIN"
  echo "$output" | grep -qE "^graph-summary\|[0-9A-Z]+\|(clean|dirty|unreachable)$"
  echo "$output" | grep -qE "^graph-issue\|[a-z-]+\|"
}
