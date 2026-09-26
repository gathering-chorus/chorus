#!/usr/bin/env bats
# @test-type: integration — creates the in-memory test dataset in the running Fuseki
# @domain: tests — the product domain this suite guards (#4334)
#
# #4332 — the dataset bats suites write to is never /pods. These prove the
# helper every writing suite now loads (lib/test-store.sh): it hands out a
# dataset that is not production, it refuses to be pointed at production,
# and a write through it lands there and not in /pods.

setup() {
  . "$BATS_TEST_DIRNAME/lib/test-store.sh"
}

@test "test_store exports endpoints on a dataset that is not /pods" {
  test_store || skip "UNMEASURED: $TEST_STORE_WHY"
  case "$FUSEKI_GSP$FUSEKI_QUERY$FUSEKI_UPDATE" in
    */pods/*) echo "an endpoint points at /pods: $FUSEKI_GSP $FUSEKI_QUERY $FUSEKI_UPDATE"; return 1 ;;
  esac
  test -n "$TEST_STORE"
}

@test "a write through the test store lands there, and /pods never sees it" {
  test_store || skip "UNMEASURED: $TEST_STORE_WHY"
  g="urn:chorus:test:4332-probe-$$-$RANDOM"
  auth=()
  [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && auth=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  code=$(printf '<urn:x:s> <urn:x:p> "4332" .\n' | curl -s -o /dev/null -w '%{http_code}' "${auth[@]}" \
    -X PUT -H 'Content-Type: application/n-triples' --data-binary @- "$FUSEKI_GSP?graph=$g")
  test "$code" = "201" -o "$code" = "200" -o "$code" = "204"
  here=$(curl -s "${auth[@]}" -G "$FUSEKI_QUERY" -H 'Accept: text/csv' \
    --data-urlencode "query=ASK { GRAPH <$g> { ?s ?p ?o } }" | tail -1 | tr -d '\r')
  there=$(curl -s "${auth[@]}" -G "http://localhost:3030/pods/query" -H 'Accept: text/csv' \
    --data-urlencode "query=ASK { GRAPH <$g> { ?s ?p ?o } }" | tail -1 | tr -d '\r')
  curl -s -o /dev/null "${auth[@]}" -X DELETE "$FUSEKI_GSP?graph=$g" || true
  test "$here" = "true"
  test "$there" = "false"
}

@test "NEGATIVE PROOF: test_store refuses to be pointed at /pods" {
  run env CHORUS_TEST_STORE=pods bash -c ". '$BATS_TEST_DIRNAME/lib/test-store.sh'; test_store; echo \"rc=\$? \$TEST_STORE_WHY\""
  echo "$output" | grep -q "rc=3 CHORUS_TEST_STORE=pods is the production dataset"
}

@test "NEGATIVE PROOF: assert_not_prod refuses a /pods write URL and passes the test store" {
  run assert_not_prod "http://localhost:3030/pods/update"
  test "$status" -ne 0
  echo "$output" | grep -q "production dataset"
  run assert_not_prod "http://localhost:3030/chorus-test/update"
  test "$status" -eq 0
}

@test "NEGATIVE PROOF: a store that does not answer is UNMEASURED, never a green" {
  run env FUSEKI_BASE_URL="http://127.0.0.1:9" bash -c ". '$BATS_TEST_DIRNAME/lib/test-store.sh'; test_store; echo \"rc=\$? \$TEST_STORE_WHY\""
  echo "$output" | grep -q "rc=2 Fuseki unreachable"
}
