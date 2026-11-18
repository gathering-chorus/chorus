#!/usr/bin/env bats
# @test-type: integration:api
# @domain: identity — the product domain this suite guards (#4334)
# #4220 — the two claims this card makes about the LIVE door, run against the
# door rather than described in a demo. Jeff, 2026-09-19: presenting with "one
# honest note" attached is a tell, and the note that day was that prove-live had
# nothing to run — the proofs existed only as commands I typed by hand.
#
# Asserts are simple commands. `[[ ]]` mid-block passes on bash 3.2 whatever it
# claims, and `! cmd` under set -e does the same (#4213).

API="${ATHENA_MAKE_URL:-http://localhost:3360}"
ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/.." && pwd)"

setup() {
  curl -sf --max-time 5 "$API/health" >/dev/null || skip "athena-make not up at $API"
  HDR="$BATS_TEST_TMPDIR/h.txt"
  tok="$(bash "$ROOT/scripts/chorus-identity-token" silas 2>/dev/null | tr -d '\n')"
  [ -n "$tok" ] || skip "no credential for silas on this box"
  printf 'Authorization: Bearer %s\n' "$tok" > "$HDR"
  chmod 600 "$HDR"
}

# #4409 — every row a test writes is named here and deleted in teardown, so a
# failed assert mid-test cannot leave it in the live store.
teardown() {
  [ -f "$BATS_TEST_TMPDIR/created" ] || return 0
  while read -r name; do
    [ -n "$name" ] && curl -s --max-time 10 -X DELETE -H "@$HDR" "$API/v1/identity/sessionruns/$name" -o /dev/null
  done < "$BATS_TEST_TMPDIR/created"
}

FUSEKI_QUERY_URL="${FUSEKI_QUERY:-http://localhost:3030/pods/sparql}"

# #5002 — since #4517 only the door writes Session rows, so the "a class it owns" proof writes a
# SessionRun (identity graph, silas holds Append there; its turns write these) on silas's own
# open session, read from the door.
silas_session() {
  curl -sf --max-time 20 "$API/v1/identity/sessions?endedAt.absent=true&channel=pane&limit=500" \
    | python3 -c "import json,sys; print(next((r['name'] for r in json.load(sys.stdin)['data'] if (r.get('ownedBy') or '').endswith('principal-silas')), ''))"
}

@test "#4220 a session may NOT create a Principal — who exists is deploy-only" {
  run curl -s --max-time 10 -X POST -H 'Content-Type: application/json' -H "@$HDR" \
    --data '{"name":"bats-4220-principal","label":"p","principalKind":"agent","canSignIn":"false"}' \
    "$API/v1/identity/principals"
  echo "$output"
  printf '%s' "$output" | grep -q "deploy-only"
  # and nothing was created under that name
  run curl -s --max-time 10 "$API/v1/identity/principals/bats-4220-principal"
  test -z "$(printf '%s' "$output" | grep -F '"webId"' || true)"
}

@test "#4220 NEGATIVE PROOF: the same caller CAN still write a class it owns" {
  # If this fails, the refusal above proves nothing about deploy-only — it would
  # just mean the caller cannot write anything at all.
  sess="$(silas_session)"; [ -n "$sess" ] || skip "silas has no open pane session to run under"
  name="bats-4220-run-$$"
  echo "$name" >> "$BATS_TEST_TMPDIR/created"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -H "@$HDR" \
    --data "{\"name\":\"$name\",\"label\":\"bats\",\"comment\":\"bats 4220\",\"conversationId\":\"bats-4220-$$\",\"runOf\":\"$sess\",\"startedAt\":\"2026-01-01T00:00:00Z\"}" \
    "$API/v1/identity/sessionruns"
  test "$output" = "201"

  # and its own row can be ended
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X PATCH -H 'Content-Type: application/json' -H "@$HDR" \
    --data '{"runEndedAt":"2026-01-01T00:05:00Z"}' "$API/v1/identity/sessionruns/$name"
  test "$output" = "200"
  run curl -s --max-time 10 "$API/v1/identity/sessionruns/$name"
  printf '%s' "$output" | grep -q '"runEndedAt"'
}

@test "#4409 the row the door test writes is gone after teardown" {
  sess="$(silas_session)"; [ -n "$sess" ] || skip "silas has no open pane session to run under"
  name="bats-4220-residue-$$"
  echo "$name" >> "$BATS_TEST_TMPDIR/created"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -H "@$HDR" \
    --data "{\"name\":\"$name\",\"label\":\"bats\",\"comment\":\"bats 4220\",\"conversationId\":\"bats-4220r-$$\",\"runOf\":\"$sess\",\"startedAt\":\"2026-01-01T00:00:00Z\"}" \
    "$API/v1/identity/sessionruns"
  test "$output" = "201"
  teardown
  : > "$BATS_TEST_TMPDIR/created"
  # NEGATIVE PROOF that the read can see a row: it saw this one before teardown ran
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$API/v1/identity/sessionruns/$name"
  test "$output" = "404"
}

@test "#4220 the door names the graph the MODEL declares, not a built-in" {
  # #4256 — this asserted the literal string "urn:chorus:domains:identity",
  # which is the built-in it exists to forbid. #4226 moved PrincipalShape's
  # instancesGraph to security (the home follows the rows: all 12 Principals
  # are there), and the test went red while the door was doing exactly what
  # the title asks. Ask the model what it declares, then require the door to
  # agree with THAT.
  declared="$(curl -s --max-time 10 --data-urlencode \
    'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?g WHERE { GRAPH <urn:chorus:ontology> { ?s sh:targetClass chorus:Principal ; chorus:instancesGraph ?g } } LIMIT 1' \
    -H 'Accept: text/csv' "$FUSEKI_QUERY_URL" | tail -1 | tr -d '\r')"
  [ -n "$declared" ] || skip "model unreadable here — UNMEASURED, not green"
  run curl -s --max-time 10 "$API/v1/identity/principals"
  printf '%s' "$output" | grep -q "$declared"
}

@test "#4220 NEGATIVE PROOF — the door does NOT name the graph it used to hardcode" {
  # The code gate caught my first attempt: it grepped for
  # urn:chorus:domains:not-a-real-home, a string that could never appear
  # whatever the door did, so it proved nothing. The real failure this guards
  # is the ONE that actually happened — the door naming urn:chorus:domains:identity
  # while PrincipalShape declares security. That is a plausible wrong answer,
  # it was the literal the test asserted until today, and if the door regressed
  # to it this assertion fails.
  declared="$(curl -s --max-time 10 --data-urlencode \
    'query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT ?g WHERE { GRAPH <urn:chorus:ontology> { ?s sh:targetClass chorus:Principal ; chorus:instancesGraph ?g } } LIMIT 1' \
    -H 'Accept: text/csv' "$FUSEKI_QUERY_URL" | tail -1 | tr -d '\r')"
  [ -n "$declared" ] || skip "model unreadable here — UNMEASURED, not green"
  # the wrong-but-plausible candidate: whichever of the two the model did NOT pick
  if [ "$declared" = "urn:chorus:domains:identity" ]; then
    wrong="urn:chorus:domains:security"
  else
    wrong="urn:chorus:domains:identity"
  fi
  run curl -s --max-time 10 "$API/v1/identity/principals"
  printf '%s' "$output" | grep -q "$declared"
  ! printf '%s' "$output" | grep -q "$wrong" || return 1
}
