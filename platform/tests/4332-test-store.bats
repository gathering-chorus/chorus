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
  curl -s --max-time 10 -o /dev/null "${auth[@]}" -X DELETE "$FUSEKI_GSP?graph=$g" || true
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

# #4416 reopen 2 — 10-14: a private door's dataset was left in the running
# Fuseki (chorus-door-88840-32658, 15,928 triples) and the stores sensor went
# red on it. A killed run never reaches teardown, so the next door sweeps, and
# a stop that cannot delete says so.
door_ds() { # $1 name — make an in-memory dataset the way private_door does
  . "$BATS_TEST_DIRNAME/../scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
  AUTH=(); [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && AUTH=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  BASE="${FUSEKI_BASE_URL:-http://localhost:3030}"
  curl -sf --max-time 10 -o /dev/null "${AUTH[@]}" -X POST "$BASE/\$/datasets" --data "dbName=$1&dbType=mem" \
    || skip "UNMEASURED: could not create a dataset in Fuseki at $BASE"
}
ds_exists() { curl -sf --max-time 10 -o /dev/null "${AUTH[@]}" "$BASE/\$/datasets/$1"; }
ds_drop() { curl -s --max-time 10 -o /dev/null "${AUTH[@]}" -X DELETE "$BASE/\$/datasets/$1"; }

@test "a door whose run is gone is swept by the next door; a live run's door is kept" {
  sleep 0 & local dead=$!; wait "$dead"
  door_ds "chorus-door-$dead-1"
  door_ds "chorus-door-$$-2"
  private_door_sweep "$BASE" "${AUTH[@]}"
  local orphan_left=0 live_left=0
  ds_exists "chorus-door-$dead-1" && orphan_left=1
  ds_exists "chorus-door-$$-2" && live_left=1
  ds_drop "chorus-door-$dead-1"; ds_drop "chorus-door-$$-2"
  [ "$orphan_left" -eq 0 ] || { echo "the orphan survived the sweep"; return 1; }
  [ "$live_left" -eq 1 ] || { echo "the sweep deleted a live run's door"; return 1; }
}

@test "NEGATIVE PROOF: a stop that cannot delete its dataset is red and names it, never silent" {
  door_ds "chorus-door-$$-3"
  run env FUSEKI_ADMIN_PASSWORD=not-the-password PRIVATE_DOOR_DS="chorus-door-$$-3" \
    bash -c ". '$BATS_TEST_DIRNAME/lib/test-store.sh'; private_door_stop"
  local left=0; ds_exists "chorus-door-$$-3" && left=1
  ds_drop "chorus-door-$$-3"
  [ "$status" -ne 0 ] || { echo "the stop returned 0 with the dataset left: $output"; return 1; }
  [[ "$output" == *"chorus-door-$$-3 NOT deleted"* ]] || return 1
  [ "$left" -eq 1 ] || return 1
}

@test "a stop with the credentials deletes its dataset and returns 0" {
  door_ds "chorus-door-$$-4"
  run env -u FUSEKI_ADMIN_PASSWORD PRIVATE_DOOR_DS="chorus-door-$$-4" \
    bash -c ". '$BATS_TEST_DIRNAME/lib/test-store.sh'; private_door_stop"
  local left=0; ds_exists "chorus-door-$$-4" && left=1
  ds_drop "chorus-door-$$-4"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$left" -eq 0 ] || return 1
}
