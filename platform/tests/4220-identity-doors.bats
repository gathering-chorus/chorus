#!/usr/bin/env bats
# @test-type: integration:api
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

FUSEKI_QUERY_URL="${FUSEKI_QUERY:-http://localhost:3030/pods/sparql}"

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
  name="bats-4220-sess-$$"
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -H "@$HDR" \
    --data "{\"name\":\"$name\",\"label\":\"bats\",\"ownedBy\":\"principal-silas\",\"tokenId\":\"bats-4220-$$\",\"issuedAt\":\"2026-01-01T00:00:00Z\",\"expiresAt\":\"2026-01-01T00:10:00Z\",\"sessionState\":\"open\",\"hostAccount\":\"bats\"}" \
    "$API/v1/identity/sessions"
  test "$output" = "201"

  # #4220 — and a session can be ENDED, which nothing could do before this card.
  run curl -s --max-time 10 -X PUT -H 'Content-Type: application/json' -H "@$HDR" \
    --data "{\"label\":\"bats\",\"tokenId\":\"bats-4220-$$\",\"issuedAt\":\"2026-01-01T00:00:00Z\",\"expiresAt\":\"2026-01-01T00:10:00Z\",\"sessionState\":\"closed\",\"endedAt\":\"2026-01-01T00:05:00Z\",\"hostAccount\":\"bats\"}" \
    "$API/v1/identity/sessions/$name"
  printf '%s' "$output" | grep -q '"ok"'
  run curl -s --max-time 10 "$API/v1/identity/sessions/$name"
  printf '%s' "$output" | grep -q '"endedAt"'
  printf '%s' "$output" | grep -q 'closed'

  curl -s --max-time 10 -X DELETE -H "@$HDR" "$API/v1/identity/sessions/$name" -o /dev/null
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

@test "#4220 NEGATIVE PROOF — the door would fail if it named a different graph" {
  # The check above can only mean something if a wrong graph name fails it.
  run curl -s --max-time 10 "$API/v1/identity/principals"
  ! printf '%s' "$output" | grep -q "urn:chorus:domains:not-a-real-home"
}
